//! YouTube playback: one `mpv` child process driven over its JSON IPC socket.
//!
//! Verified against mpv 0.42.0 and yt-dlp before this module was written: a `list=RD` mix URL does
//! enumerate the seed plus its recommendations through `yt-dlp --flat-playlist`, but mpv does NOT
//! expand that mix into its own playlist (`playlist-count` stays 1, so `playlist-next` fails).
//! The queue therefore lives here: yt-dlp enumerates the recommendations, and each entry is loaded
//! into mpv explicitly by id.
//!
//! The video is drawn by mpv in its own window. This module only sends commands and reports state.

use crate::core::Event;
use crate::media::{CellRect, MediaCmd, Video, Youtube};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;

/// Opt-in trace for the video window (`GS3_HUD_DEBUG=1`), appended to `$TMPDIR/gs3-hud-<pid>-debug.log`.
/// The visibility rule mixes four inputs that no unit test can observe, so a report of "the video
/// disappeared" is only diagnosable from what the engine actually decided.
fn trace(args: std::fmt::Arguments) {
    if std::env::var_os("GS3_HUD_DEBUG").is_none() { return }
    let path = std::env::temp_dir().join(format!("gs3-hud-{}-debug.log", std::process::id()));
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let _ = writeln!(file, "{ms} {args}");
    }
}
macro_rules! trace { ($($arg:tt)*) => { trace(format_args!($($arg)*)) } }
use std::thread;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(500);
/// The visibility tick: the window must follow focus quickly, and reading an atomic costs nothing.
const TICK: Duration = Duration::from_millis(80);
/// How many consecutive ticks the video block has to stay missing before a playing window is hidden.
/// A closing modal redraws one frame without the block, and hiding for that minimizes the window while
/// mpv is still resolving its URL, which is exactly when the window is born in the wrong state.
const HIDE_TICKS: u8 = 5;
/// How often the window state is read back. The property drifts after it is set: mpv creates the window
/// only once the URL resolves, so a minimize that arrived before that survives the later deminiaturize.
const VERIFY: Duration = Duration::from_millis(400);
const QUEUE_MAX: usize = 24;
const LISTS: usize = 10;
const LIST_CAP: usize = 200;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// Parse the bounds CoreGraphics reports for a window, rejecting anything incomplete.
fn parse_window_json(reply: &str) -> Option<(i64, i64, i64, i64)> {
    let value: Value = serde_json::from_str(reply.trim()).ok()?;
    let n = |key: &str| value[key].as_f64().map(|f| f as i64);
    let (x, y, width, height) = (n("X")?, n("Y")?, n("Width")?, n("Height")?);
    (width > 0 && height > 0).then_some((x, y, width, height))
}

fn parse_layout(reply: &Value, id: &str) -> Option<(CellRect, CellRect)> {
    fn rect(v: &Value) -> Option<CellRect> {
        let n = |key| u32::try_from(v[key].as_u64()?).ok();
        let r = CellRect::new(n("x")?, n("y")?, n("width")?, n("height")?);
        (r.width > 0 && r.height > 0).then_some(r)
    }
    let layout = &reply["layout"];
    let area = rect(&layout["area"])?;
    let pane = layout["panes"].as_array()?.iter().find(|v| v["pane_id"].as_str() == Some(id))?;
    Some((area, rect(&pane["rect"])?))
}

/// The text grid inside the terminal's window frame, in the pixels a window list reports: where its
/// first cell starts, how big a cell is, and how many cells the multiplexer spends before the layout
/// area's own origin.
///
/// The frame is *not* the grid. macOS puts a title bar above the grid and the terminal pads around it,
/// and none of that is a cell, so scaling cells into the frame put every row several pixels too high:
/// measured on the rail, a block at area row 21 landed 48 px above its own cells — over the rows above
/// it and short of the rows below — and 15% too large at both axes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Grid {
    x: i64, y: i64, cell_w: i64, cell_h: i64,
    skip_x: i64, skip_y: i64, inset_x: i64, inset_y: i64,
}

/// The terminal window's screen rectangle, as a window list reports it.
type Frame = (i64, i64, i64, i64);

/// This pane's own grid, as its terminal reports it: how big a cell is, and how many of them it draws.
/// The pane's layout rectangle is two cells bigger on each axis than this, because the multiplexer
/// spends a cell of border on every side of a pane and the program inside only sees the content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Cells { w: i64, h: i64, cols: i64, rows: i64 }

/// How much of the window sits above the first row of the grid. Measured on Ghostty with its tab bar;
/// `GS3_HUD_GRID_TOP` overrides it for a terminal whose chrome is a different height.
const GRID_TOP: i64 = 38;

/// The size of one cell of this pane, in the pixels the window list reports. The terminal answers with
/// the grid it is drawing, so this needs no font, no scale factor and no permission. Some terminals
/// leave the pixel half of the answer empty; a cell is a few pixels wide and a few more tall.
fn cells_px() -> Option<Cells> {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } != 0 { return None }
    let (cols, rows) = (i64::from(ws.ws_col), i64::from(ws.ws_row));
    let (px, py) = (i64::from(ws.ws_xpixel), i64::from(ws.ws_ypixel));
    if cols <= 0 || rows <= 0 || px <= 0 || py <= 0 { return None }
    let (w, h) = (px / cols, py / rows);
    ((3..=64).contains(&w) && (6..=128).contains(&h)).then_some(Cells { w, h, cols, rows })
}

/// Where the layout's cells are on screen, from the window frame and this pane's own grid.
///
/// Without that grid the frame is all there is, and the area is scaled into the whole of it: that is
/// what this did before the grid was measured. It is wrong by the chrome, and better than nothing.
fn grid_of(frame: Frame, area: CellRect, pane: CellRect, cells: Option<Cells>) -> Grid {
    let (fx, fy, fw, fh) = frame;
    let area_w = i64::from(area.width.max(1)); let area_h = i64::from(area.height.max(1));
    let Some(c) = cells.filter(|c| c.w > 0 && c.h > 0) else {
        return Grid { x: fx, y: fy, cell_w: (fw / area_w).max(1), cell_h: (fh / area_h).max(1), skip_x: 0, skip_y: 0, inset_x: 0, inset_y: 0 }
    };
    let top = std::env::var("GS3_HUD_GRID_TOP").ok().and_then(|v| v.parse::<i64>().ok()).unwrap_or(GRID_TOP);
    let cols = (fw / c.w).max(1);
    let rows = ((fh - top).max(c.h) / c.h).max(1);
    Grid {
        // The frame is a few pixels wider than the whole cells in it, padded on both sides.
        x: fx + (fw - cols * c.w) / 2, y: fy + top, cell_w: c.w, cell_h: c.h,
        skip_x: (cols - area_w).max(0), skip_y: (rows - area_h).max(0),
        inset_x: (i64::from(pane.width) - c.cols).max(0) / 2,
        inset_y: (i64::from(pane.height) - c.rows).max(0) / 2,
    }
}

fn geometry(grid: Grid, area: CellRect, pane: CellRect, card: CellRect) -> Option<String> {
    if area.width == 0 || area.height == 0 || card.width == 0 || card.height == 0 { return None }
    fn axis(origin: i64, cell: i64, skip: i64, area_start: u32, area_size: u32, pane_start: u32, pane_size: u32, start: u32, length: u32) -> Option<(i64,i64)> {
        let grid_start = i128::from(area_start); let grid_end = grid_start + i128::from(area_size);
        let pane_start = grid_start + i128::from(pane_start); let pane_end = pane_start + i128::from(pane_size);
        let begin = (pane_start + i128::from(start)).max(grid_start).min(pane_end);
        let end = (pane_start + i128::from(start) + i128::from(length)).min(grid_end).min(pane_end);
        if begin >= end { return None }
        let scale = |v: i128| i128::from(origin) + (i128::from(skip) + v) * i128::from(cell);
        let a = scale(begin); let b = scale(end);
        if b <= a { return None }
        Some((i64::try_from(a).ok()?, i64::try_from(b-a).ok()?))
    }
    // The card speaks in the coordinates of what the pane draws, which starts inside its border.
    let (px, pw) = (pane.x + u32::try_from(grid.inset_x).unwrap_or(0), pane.width.saturating_sub(u32::try_from(grid.inset_x * 2).unwrap_or(0)));
    let (py, ph) = (pane.y + u32::try_from(grid.inset_y).unwrap_or(0), pane.height.saturating_sub(u32::try_from(grid.inset_y * 2).unwrap_or(0)));
    let (x,w) = axis(grid.x,grid.cell_w,grid.skip_x,area.x,area.width,px,pw,card.x,card.width)?;
    let (y,h) = axis(grid.y,grid.cell_h,grid.skip_y,area.y,area.height,py,ph,card.y,card.height)?;
    // Rust's own sign: mpv accepts `-20` and rejects `+-20`.
    Some(format!("{w}x{h}{x:+}{y:+}"))
}

fn placement(window: Option<Frame>, layout: Option<(CellRect,CellRect)>, card: Option<CellRect>) -> Result<String, &'static str> {
    let card = card.ok_or("sin área visible de YouTube")?;
    let (area,pane) = layout.ok_or("sin ubicación del panel")?;
    let frame = window.ok_or("sin ubicación de ventana del terminal")?;
    geometry(grid_of(frame, area, pane, cells_px()), area, pane, card).ok_or("área de video inválida")
}

/// The window belongs to the app that is in front, and only to it: this rail's video is always on top,
/// so it must hide as soon as anything else comes forward. Pure, so the rule is pinned without a GUI.
fn visible_now(frontmost: Option<i64>, ours: Option<i64>, player: Option<u32>, on_our_tab: bool, card: bool) -> bool {
    // Another herdr tab, or another tab of the rail itself, means the card is not on screen, so the
    // video must not be either.
    if !on_our_tab || !card { return false }
    match (frontmost, ours) {
        // The player belongs to this rail: when it takes focus (mpv raises its own window) the video
        // is the thing being watched, so it stays. Anything else in front hides it.
        (Some(f), Some(o)) => f == o || Some(f) == player.map(i64::from),
        _ => true,
    }
}

/// One request over herdr's socket: the rail's own tab and the tab in front, so the video can follow
/// tab switches and not only app focus.
fn herdr_query(path: &std::path::Path, method: &str, params: Value) -> Option<Value> {
    let stream = UnixStream::connect(path).ok()?;
    stream.set_read_timeout(Some(REQUEST_TIMEOUT)).ok()?;
    let mut out = stream.try_clone().ok()?;
    out.write_all(json!({"id": "visibility", "method": method, "params": params}).to_string().as_bytes()).ok()?;
    out.write_all(b"\n").ok()?;
    out.flush().ok()?;
    let mut line = String::new();
    BufReader::new(stream).read_line(&mut line).ok()?;
    serde_json::from_str::<Value>(line.trim()).ok()
}

fn our_tab(path: &std::path::Path, pane: &str) -> Option<String> {
    let reply = herdr_query(path, "pane.get", json!({"pane_id": pane}))?;
    reply["result"]["pane"]["tab_id"].as_str().map(str::to_owned)
}

fn focused_tab(path: &std::path::Path) -> Option<String> {
    let reply = herdr_query(path, "session.snapshot", json!({}))?;
    reply["result"]["snapshot"]["focused_tab_id"].as_str().map(str::to_owned)
}

/// A single long-lived helper reporting the active application as it changes, so the rail reacts in
/// about a tenth of a second instead of spawning one `osascript` per check. Returns the child (kept
/// alive as long as a player exists) and the shared value.
fn spawn_focus_watch() -> Option<(Child, Arc<AtomicI64>)> {
    let script = r#"ObjC.import("AppKit");
var last = -1;
while (true) {
  var pid = Number($.NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier);
  if (pid !== last) { last = pid; console.log(String(pid)); }
  $.NSThread.sleepForTimeInterval(0.1);
}"#;
    let mut child = Command::new("osascript").args(["-l", "JavaScript", "-e", script]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let shared = Arc::new(AtomicI64::new(-1));
    let sink = Arc::clone(&shared);
    let stdout = child.stdout.take()?;
    thread::spawn(move || for line in BufReader::new(stdout).lines() {
        match line { Ok(line) => if let Some(pid) = parse_pid(&line) { sink.store(pid, Ordering::Relaxed) }, Err(_) => break }
    });
    Some((child, shared))
}

/// The active application, asked to the window server's own notion of activation. A window-list
/// heuristic cannot answer this: an always-on-top player is a window of *our* session but not the
/// active app, and `--ontop` raises it to a layer of its own (measured: layer 3, not 0).
fn frontmost_pid() -> Option<i64> {
    let script = r#"ObjC.import("AppKit");
String($.NSWorkspace.sharedWorkspace.frontmostApplication.processIdentifier);"#;
    let out = Command::new("osascript").args(["-l", "JavaScript", "-e", script]).output().ok()?;
    parse_pid(&String::from_utf8_lossy(&out.stdout))
}

/// A pid from the helper's line, or nothing when the line is not one.
fn parse_pid(line: &str) -> Option<i64> {
    line.trim().parse::<i64>().ok().filter(|p| *p > 0)
}

/// The app that owns this rail's window: the terminal at the top of the process chain.
fn terminal_pid() -> Option<i64> {
    let owners = serde_json::to_string(&ancestors()).ok()?;
    let script = format!(r#"ObjC.import("CoreGraphics");
const owners = {owners};
const list = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, 0)));
const hit = list.find(w => (w.kCGWindowLayer|0) === 0 && owners.includes(w.kCGWindowOwnerPID|0) && (w.kCGWindowBounds.Width|0) > 100);
hit ? String(hit.kCGWindowOwnerPID|0) : "";"#);
    let out = Command::new("osascript").args(["-l", "JavaScript", "-e", &script]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<i64>().ok()
}

#[derive(Debug, PartialEq, Eq)]
enum Movement { None, Rebuild }

/// Whether the window is where the card is. A block that moved needs a new window just as much as one
/// that resized, so both answer `Rebuild`: a variant that preserved the window kept it over `buscar`.
fn needs_move(used: Option<CellRect>, want: Option<CellRect>, visible: bool) -> Movement {
    if !visible || want.is_none() || used == want { return Movement::None }
    Movement::Rebuild
}

/// How long a wanted block must stay still before one rebuild is spent on it.
const SETTLE: Duration = Duration::from_millis(300);

/// Whether the wait for a wanted block has been spent. The wait restarts whenever the block asks for a
/// different place: a playlist loading or a wheel over the body shifts it several frames running, and
/// each of those frames used to spend the wait on a rebuild that was already stale.
fn settled(pending: &mut Option<(CellRect, Instant)>, want: CellRect, now: Instant) -> bool {
    match *pending {
        Some((asked, at)) if asked == want => {
            if now.saturating_duration_since(at) < SETTLE { return false }
            *pending = None;
            true
        }
        _ => { *pending = Some((want, now)); false }
    }
}

/// Put the window on the block the card is using now, moving or resizing alike: the block sits below the
/// Spotify card and below `buscar`, so anything that grows above it (a playlist, a search result, an
/// expanded card) moves it, and a window left where the block used to be covers the search row.
///
/// mpv's window cannot be repositioned in place: its runtime `geometry` property is a no-op on this
/// build and it has no move command, and the only permission-free way to place a window of another
/// process is to start it there. So a moved block gets a fresh window at the same video and the same
/// position, debounced so a card that settles in several frames is rebuilt once.
fn move_window(player: &mut Option<Player>, y: &mut Youtube, card: Option<CellRect>, visible: bool, moved_at: &mut Option<(CellRect, Instant)>, socket: &std::path::Path) {
    let Some(p) = player.as_mut() else { *moved_at = None; return };
    match needs_move(p.geom, card, visible) {
        Movement::None => { *moved_at = None; return }
        Movement::Rebuild => {}
    }
    let Some(want) = card else { *moved_at = None; return };
    if !settled(moved_at, want, Instant::now()) { return }
    let used = p.geom;
    // Record the block this rebuild is for before anything can fail. A return path that left the old
    // value behind rebuilds again on the next tick, and a rebuild every tick killed and respawned mpv
    // in a loop that took the keyboard away from the rail: the whole sidebar looked frozen.
    p.geom = card;
    let Some(id) = y.queue.get(y.index).map(|v| v.id.clone()) else { return };
    let position = y.position;
    let layout = std::env::var("HERDR_PANE_ID").ok().and_then(|pane| super::request(socket, "pane.layout", json!({"pane_id": pane})).ok().and_then(|v| parse_layout(&v, &pane)));
    let geometry = placement(window_rect(), layout, card);
    trace!("move: used={used:?} want={card:?} geometry={:?}", geometry.as_deref().ok());
    drop(player.take());
    let Some(mut fresh) = Player::start(geometry.as_deref().ok(), card) else { y.state = "mpv no encontrado".into(); return };
    if fresh.load(&id) {
        if position > 0.5 { fresh.call(&["seek", &format!("{position:.3}"), "absolute"]); }
        *player = Some(fresh);
    } else { y.state = "no se pudo recolocar el video".into() }
}

/// Whether the window may be on screen right now. Showing is immediate; hiding waits until the block has
/// been gone for a few consecutive ticks, because one frame without it is a redraw, not the card leaving.
fn follow(missing: &mut u8, raw: bool) -> bool {
    if raw { *missing = 0; return true }
    *missing = missing.saturating_add(1);
    *missing < HIDE_TICKS
}

/// Tell the player whether its window may be on screen, and keep it there: the wanted state is written
/// once, but mpv's window state is read back on a slower cadence and re-asserted whenever it drifted.
/// A one-shot confirmation is not enough (a window created seconds later, or a minimize that lands
/// while the URL is still resolving, answers success and then disagrees), and the failure mode is the
/// worst one: the video vanishes while its audio keeps playing.
fn apply_visibility(p: &mut Player, shown: &mut Option<(u32, bool)>, visible: bool, verify: bool) {
    let key = (p.child.id(), visible);
    let want = window_value(visible);
    if *shown == Some(key) {
        if !verify { return }
        let actual = p.call(&["get_property", "window-minimized"]).and_then(|reply| reply["data"].as_bool());
        if actual == Some(want) { return }
        trace!("visibility drifted pid={} want={want} actual={actual:?}", key.0);
    }
    if p.set_bool("window-minimized", want).is_none() { return }
    let got = p.call(&["get_property", "window-minimized"]).and_then(|reply| reply["data"].as_bool());
    trace!("visibility want_visible={visible} pid={} asked={want} confirmed={got:?}", key.0);
    if got == Some(want) { *shown = Some(key); } else { *shown = None }
}

/// The value mpv's `window-minimized` takes for the requested visibility.
fn window_value(visible: bool) -> bool { !visible }

fn mpv_args(socket: &str, geometry: Option<&str>) -> Vec<String> {
    let mut args = vec!["--idle=yes".into(), "--no-terminal".into(), "--msg-level=all=error".into(), "--keep-open=yes".into(), "--title=gs3 media".into(), format!("--input-ipc-server={socket}"), "--border=no".into(), "--osc=no".into(), "--ontop".into(), "--panscan=1.0".into(), "--macos-app-activation-policy=accessory".into()];
    // `whole` because the rectangle we compute comes from CoreGraphics, which reports absolute screen
    // coordinates: with mpv's default `visible` calculation the window lands a menu bar lower.
    // `whole` because the rectangle comes from CoreGraphics, which reports absolute screen
    // coordinates: mpv's default `visible` calculation lands a menu bar lower. `force-window` because
    // otherwise the window is created when the file loads, and mpv then applies the geometry shifted
    // (measured: asked +1488+406, landed at +1513+438).
    if let Some(g) = geometry {
        args.push(format!("--geometry={g}"));
        args.push("--macos-geometry-calculation=whole".into());
        args.push("--force-window=yes".into());
    }
    args
}

/// This rail's process chain, nearest first. The window belongs to the terminal app at the top, not
/// to the pane, and `--pane` names the agent pane, so neither is a substitute.
fn ancestors() -> Vec<i64> {
    let mut chain = Vec::new();
    let mut pid = i64::from(std::process::id());
    for _ in 0..10 {
        chain.push(pid);
        let parent = Command::new("ps").args(["-o", "ppid=", "-p", &pid.to_string()]).output().ok()
            .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse::<i64>().ok());
        match parent { Some(p) if p > 1 => pid = p, _ => break }
    }
    chain
}

/// The terminal window's screen rectangle, read through CoreGraphics in JXA. This is the piece that
/// used to need Accessibility: only a window's *title* is protected, its bounds are not, so asking for
/// geometry costs no permission and works from a detached herdr server.
fn window_rect() -> Option<(i64,i64,i64,i64)> {
    let owners = serde_json::to_string(&ancestors()).ok()?;
    let script = format!(r#"ObjC.import("CoreGraphics");
const owners = {owners};
const list = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, 0)));
const hit = list.find(w => (w.kCGWindowLayer|0) === 0 && owners.includes(w.kCGWindowOwnerPID|0) && (w.kCGWindowBounds.Width|0) > 100);
JSON.stringify(hit ? hit.kCGWindowBounds : null);"#);
    let out = Command::new("osascript").args(["-l", "JavaScript", "-e", &script]).output().ok()?;
    if !out.status.success() { return None }
    parse_window_json(&String::from_utf8_lossy(&out.stdout))
}

/// A Homebrew installation is not always on the PATH the rail inherits from its launcher.
pub(super) fn find_tool(name: &str) -> Option<PathBuf> {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from));
    dirs.into_iter().map(|d| d.join(name)).find(|p| p.is_file())
}

fn tool_path() -> String {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    for d in ["/opt/homebrew/bin", "/usr/local/bin"] { if !dirs.iter().any(|p| p == std::path::Path::new(d)) { dirs.push(PathBuf::from(d)); } }
    std::env::join_paths(dirs).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "/usr/bin:/bin".into())
}

/// Suppress stale EOF after loading and consume each observed end at most once.
#[derive(Default)]
enum EofGuard { #[default] Waiting, Armed, Handled }

impl EofGuard {
    /// A new load must first report a non-EOF state before its end can be consumed.
    fn observe(&mut self, eof: bool) -> bool {
        match (&*self, eof) {
            (Self::Waiting, false) => { *self = Self::Armed; false }
            (Self::Armed, true) => { *self = Self::Handled; true }
            _ => false,
        }
    }
}

/// Pick the next queued entry, or signal that the queue needs refilling.
fn next_index(list: &[Video], index: usize) -> Option<usize> {
    index.checked_add(1).filter(|&next| next < list.len())
}

/// Skip the just-finished video when selecting from a refreshed mix.
fn refill_index(list: &[Video], finished: &str) -> Option<usize> {
    list.iter().position(|video| video.id != finished)
}

/// One `mpv` process and its IPC socket. Dropping it stops the player.
struct Player { child: Child, out: UnixStream, inp: BufReader<UnixStream>, id: u64, eof: EofGuard, geom: Option<CellRect> }

impl Player {
    /// `geom` is the block this player is being started for and is required, so no caller can build a
    /// player that has forgotten it and then be rebuilt forever by the follower.
    fn start(geometry: Option<&str>, geom: Option<CellRect>) -> Option<Player> {
        let mpv = find_tool("mpv")?;
        let sock = std::env::temp_dir().join(format!("gs3-hud-{}-mpv.sock", std::process::id()));
        let _ = std::fs::remove_file(&sock);
        let child = Command::new(mpv)
            // Keep EOF observable until the engine has polled and loaded the next video.
            .args(mpv_args(&sock.to_string_lossy(), geometry))
            // mpv runs yt-dlp as its own child: it needs the same augmented PATH we resolve from.
            .env("PATH", tool_path())
            .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null())
            .spawn().ok()?;
        let mut connected = None;
        for _ in 0..50 {
            if let Ok(stream) = UnixStream::connect(&sock) { connected = Some(stream); break }
            thread::sleep(Duration::from_millis(100));
        }
        let out = connected?;
        out.set_read_timeout(Some(REQUEST_TIMEOUT)).ok()?;
        let inp = BufReader::new(out.try_clone().ok()?);
        trace!("start: pid={} geometry={geometry:?} follows={geom:?}", child.id());
        Some(Player { child, out, inp, id: 0, eof: EofGuard::default(), geom })
    }

    /// Send one JSON IPC command and wait for the reply carrying its `request_id`.
    /// Replies interleave with mpv's own events, so a plain read is not enough.
    /// Bool properties need a real boolean: mpv answers `unsupported format for accessing property`
    /// to the string "true", which a `&[&str]` command cannot express.
    fn set_bool(&mut self, name: &str, value: bool) -> Option<Value> {
        self.send(json!({"command": ["set_property", name, value], "request_id": self.id + 1}))
    }

    fn call(&mut self, command: &[&str]) -> Option<Value> {
        self.send(json!({"command": command, "request_id": self.id + 1}))
    }

    fn send(&mut self, payload: Value) -> Option<Value> {
        self.id += 1;
        let id = self.id;
        self.out.write_all(payload.to_string().as_bytes()).ok()?;
        self.out.write_all(b"\n").ok()?;
        self.out.flush().ok()?;
        loop {
            let mut line = String::new();
            if self.inp.read_line(&mut line).ok()? == 0 { return None }
            let value: Value = serde_json::from_str(line.trim()).ok()?;
            if value["request_id"].as_u64() == Some(id) { return Some(value) }
        }
    }

    /// `None` for an unavailable or null property: on idle mpv reports no position at all.
    fn get(&mut self, name: &str) -> Option<Value> {
        let reply = self.call(&["get_property", name])?;
        (reply["error"].as_str() == Some("success")).then(|| reply["data"].clone()).filter(|d| !d.is_null())
    }

    fn load(&mut self, video: &str) -> bool {
        let success = self.call(&["loadfile", &format!("https://www.youtube.com/watch?v={video}"), "replace"])
            .is_some_and(|reply| reply["error"].as_str() == Some("success"));
        // `--keep-open=yes` leaves mpv paused at EOF, and `loadfile` inherits that: without clearing it
        // every track after the first one loads paused at 0:00 while the card still said "reproduciendo"
        // (seen live: `3/24`, `▶`, `0:00 / 3:51`). A load starts playing, always.
        if success { self.call(&["set_property", "pause", "no"]); }
        self.eof = if success { EofGuard::Waiting } else { EofGuard::Handled };
        success
    }
}

impl Drop for Player {
    fn drop(&mut self) { let _ = self.child.kill(); let _ = self.child.wait(); }
}

fn run(program: &PathBuf, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program).args(args).env("PATH", tool_path()).output().map_err(|e| e.to_string())?;
    if !out.status.success() { return Err(format!("{} falló", program.display())) }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn encode(query: &str) -> String {
    let mut encoded = String::new();
    for byte in query.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write as _;
            let _ = write!(encoded, "%{byte:02X}");
        }
    }
    encoded
}

fn playlists_url(query: &str) -> String {
    format!("https://www.youtube.com/results?search_query={}&sp=EgIQAw%253D%253D", encode(query))
}
fn playlist_request(id: &str) -> (String, usize) {
    (format!("https://www.youtube.com/playlist?list={}", encode(id)), LIST_CAP)
}
fn parse_pairs(out: &str) -> Vec<(String, String)> {
    out.lines().filter_map(|line| {
        let (id, title) = line.split_once('\t')?;
        let (id, title) = (id.trim(), title.trim());
        (!id.is_empty() && !title.is_empty()).then(|| (id.into(), title.into()))
    }).collect()
}
fn list_request(url: &str, cap: usize) -> Result<Vec<(String, String)>, String> {
    let tool = find_tool("yt-dlp").ok_or("yt-dlp no encontrado")?;
    let out = run(&tool, &["--flat-playlist", "--no-warnings", "--playlist-end", &cap.to_string(), "--print", "%(id)s\t%(title)s", url])?;
    Ok(parse_pairs(&out).into_iter().take(cap).collect())
}
fn search_playlists(query: &str) -> Result<Vec<crate::media::youtube::Playlist>, String> {
    Ok(list_request(&playlists_url(query), LISTS)?.into_iter().map(|(id, title)| crate::media::youtube::Playlist { id, title }).collect())
}
fn playlist_videos(id: &str) -> Result<Vec<Video>, String> {
    let (url, cap) = playlist_request(id);
    Ok(list_request(&url, cap)?.into_iter().map(|(id, title)| Video { id, title }).collect())
}
fn apply_playlists(y: &mut Youtube, result: Result<Vec<(String, String)>, String>) {
    y.playlists.clear(); y.playlist_search_done = true; y.selected = None; y.videos.clear();
    match result {
        Ok(list) => { y.playlists = list.into_iter().map(|(id, title)| crate::media::youtube::Playlist { id, title }).collect(); y.state = if y.playlists.is_empty() { "sin listas".into() } else { format!("{} listas", y.playlists.len()) }; }
        Err(e) => y.state = e,
    }
}
fn apply_videos(y: &mut Youtube, id: &str, result: Result<Vec<Video>, String>) {
    y.selected = None; y.videos.clear();
    match result {
        Ok(videos) => { y.selected = y.playlists.iter().position(|p| p.id == id); y.videos = videos; y.state = format!("{} videos", y.videos.len()); }
        Err(e) => y.state = e,
    }
}
#[derive(Debug, PartialEq)]
enum EofDecision { Next, Stop, Refill }
fn eof_decision(list: &[Video], index: usize, from_playlist: bool) -> EofDecision {
    if next_index(list, index).is_some() { EofDecision::Next } else if from_playlist { EofDecision::Stop } else { EofDecision::Refill }
}

/// The seed result, then its YouTube recommendations (the RD mix), both enumerated by yt-dlp.
fn queue(query: &str) -> Result<Vec<Video>, String> {
    let ytdlp = find_tool("yt-dlp").ok_or("yt-dlp no encontrado")?;
    let seed = run(&ytdlp, &["--flat-playlist", "--no-warnings", "--print", "%(id)s", &format!("ytsearch1:{query}")])?;
    let id = seed.lines().map(str::trim).find(|l| !l.is_empty()).ok_or("sin resultados")?.to_string();
    let mix = format!("https://www.youtube.com/watch?v={id}&list=RD{id}");
    let out = run(&ytdlp, &["--flat-playlist", "--no-warnings", "--playlist-end", &QUEUE_MAX.to_string(), "--print", "%(id)s\t%(title)s", &mix])?;
    let mut list: Vec<Video> = out.lines().filter_map(|line| {
        let (video, title) = line.split_once('\t')?;
        let (video, title) = (video.trim(), title.trim());
        (!video.is_empty()).then(|| Video { id: video.into(), title: if title.is_empty() { video.into() } else { title.into() } })
    }).collect();
    list.sort_by_key(|v| v.id != id); // the requested track always starts the queue
    Ok(list)
}

fn advance(player: &mut Player, y: &mut Youtube, step: isize) {
    if y.queue.is_empty() { return }
    let next = (y.index as isize + step).clamp(0, y.queue.len() as isize - 1) as usize;
    if next == y.index { return }
    y.index = next;
    y.position = 0.0; y.duration = 0.0; y.title = Some(y.queue[next].title.clone());
    if !player.load(&y.queue[next].id) { y.state = "no se pudo cargar el video".into(); y.playing = false; }
}

fn ensure_player(player: &mut Option<Player>, socket: &std::path::Path, card: Option<CellRect>) -> Option<&'static str> {
    if player.is_some() { return None }
    let layout = std::env::var("HERDR_PANE_ID").ok().and_then(|id| super::request(socket, "pane.layout", json!({"pane_id":id})).ok().and_then(|v| parse_layout(&v, &id)));
    let result = placement(window_rect(), layout, card);
    let fallback = result.as_ref().err().copied();
    *player = Player::start(result.as_deref().ok(), card);
    fallback
}

fn handle(player: &mut Option<Player>, y: &mut Youtube, cmd: MediaCmd, socket: &std::path::Path, current_card: Option<CellRect>) {
    match &cmd {
        MediaCmd::SearchPlaylists(query) => {
            y.playlist_view = true; y.videos.clear(); y.selected = None;
            y.playlists.clear(); y.playlist_search_done = false;
            let result = search_playlists(query).map(|p| p.into_iter().map(|p| (p.id, p.title)).collect());
            apply_playlists(y, result); return;
        }
        MediaCmd::OpenPlaylist(id) => { let result = playlist_videos(id); apply_videos(y, id, result); return; }
        MediaCmd::PlayVideoAt(row) => {
            if *row >= y.videos.len() { return }
            let fallback = ensure_player(player, socket, current_card);
            let Some(player) = player.as_mut() else { y.state = "mpv no encontrado".into(); return };
            // `from_playlist` is not set here: it decides whether the end of the list stops or asks
            // YouTube for another mix, and a row click must not change that.
            y.queue = y.videos.clone(); y.index = *row;
            y.state = fallback.map(|reason| format!("sin geometría: {reason}")).unwrap_or_else(|| "reproduciendo".into());
            y.title = Some(y.queue[*row].title.clone()); y.position = 0.0; y.duration = 0.0;
            if !player.load(&y.queue[*row].id) { y.state = "no se pudo cargar el video".into(); y.playing = false; }
            return;
        }
        _ => {}
    }
    if let MediaCmd::Play(query, card) = &cmd {
        y.playlist_view = false; y.playlists.clear(); y.videos.clear(); y.selected = None; y.from_playlist = false; y.playlist_search_done = false;
        trace!("play query={query} card={card:?} player_exists={}", player.is_some());
        if let Some(player) = player.as_mut() { player.eof = EofGuard::Waiting; }
        y.state = format!("buscando {query}");
        match queue(query) {
            Ok(list) if !list.is_empty() => {
                let fallback = ensure_player(player, socket, *card);
                let Some(player) = player.as_mut() else { y.state = "mpv no encontrado".into(); return };
                // The mix is what the card lists, so `buscar` stops hiding its own recommendations:
                // same entries, same order, and the card's rows are what plays next.
                y.videos = list.clone();
                y.queue = list; y.index = 0; y.from_playlist = false; y.state = fallback.map(|reason| format!("sin geometría: {reason}")).unwrap_or_else(|| "reproduciendo".into());
                y.title = y.queue.first().map(|v| v.title.clone());
                y.position = 0.0; y.duration = 0.0;
                let id = y.queue[0].id.clone();
                if !player.load(&id) { y.state = "no se pudo cargar el video".into(); y.playing = false; }
            }
            Ok(_) => y.state = "sin resultados".into(),
            Err(e) => y.state = e,
        }
        return;
    }
    if let MediaCmd::CardRect(_) = cmd { return }
    let Some(player) = player.as_mut() else { y.state = "mpv no encontrado".into(); return };
    match cmd {
        MediaCmd::CardRect(_) => unreachable!("handled above"),
        MediaCmd::Toggle => { player.call(&["cycle", "pause"]); }
        MediaCmd::Next => advance(player, y, 1),
        MediaCmd::Prev => advance(player, y, -1),
        MediaCmd::Seek(fraction) => {
            if y.duration > 0.0 { player.call(&["seek", &format!("{:.3}", fraction.clamp(0.0, 1.0) * y.duration), "absolute"]); }
        }
        MediaCmd::Play(..) | MediaCmd::SearchPlaylists(..) | MediaCmd::OpenPlaylist(..) | MediaCmd::PlayVideoAt(..) => unreachable!("handled above"),
    }
}

fn poll(player: &mut Option<Player>, y: &mut Youtube) {
    let Some(player) = player.as_mut() else { return };
    if let Some(v) = player.get("duration").and_then(|v| v.as_f64()) { y.duration = v }
    if let Some(v) = player.get("time-pos").and_then(|v| v.as_f64()) { y.position = v.max(0.0) }
    if let Some(v) = player.get("pause").and_then(|v| v.as_bool()) { y.playing = !v }
    if let Some(v) = player.get("media-title").and_then(|v| v.as_str().map(str::to_owned)) { if !v.is_empty() { y.title = Some(v) } }
    let Some(eof) = player.get("eof-reached").and_then(|v| v.as_bool()) else { return };
    if !player.eof.observe(eof) {
        if eof && y.from_playlist && matches!(player.eof, EofGuard::Handled) { y.playing = false; }
        return
    }
    if let Some(finished) = y.queue.get(y.index).cloned() {
        match eof_decision(&y.queue, y.index, y.from_playlist) {
            EofDecision::Next => { advance(player, y, 1); return; }
            EofDecision::Stop => { y.playing = false; y.state = "fin de la lista".into(); return; }
            EofDecision::Refill => {}
        }
        y.playing = false;
        y.state = "buscando recomendaciones".into();
        match queue(&finished.title) {
            Ok(list) => match refill_index(&list, &finished.id) {
                Some(next) => {
                    y.queue = list;
                    y.index = next;
                    y.title = Some(y.queue[next].title.clone());
                    y.position = 0.0; y.duration = 0.0;
                    y.state = "reproduciendo".into();
                    if !player.load(&y.queue[next].id) { y.state = "no se pudo cargar el video".into(); }
                }
                None => y.state = "sin nuevas recomendaciones".into(),
            },
            Err(e) => y.state = format!("no se pudieron obtener recomendaciones: {e}"),
        }
    }
}

/// The engine thread. mpv starts lazily on the first play, so an unused rail spawns nothing.
pub fn media(tx: Sender<Event>, socket: PathBuf) -> Sender<MediaCmd> {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut youtube = Youtube { state: if find_tool("mpv").is_some() { "sin reproducción".into() } else { "mpv no encontrado".into() }, ..Youtube::default() };
        let mut player: Option<Player> = None;
        let mut sent = youtube.clone();
        let mut ours: Option<i64> = None;
        // The window's visibility is per process: a rebuilt or newly started player has to be told again.
        let mut shown: Option<(u32, bool)> = None;
        let mut watch: Option<(Child, Arc<AtomicI64>)> = None;
        let mut last_poll = Instant::now();
        let mut card: Option<CellRect> = None;
        let mut moved_at: Option<(CellRect, Instant)> = None;
        let mut missing_ticks: u8 = 0;
        let mut last_verify = Instant::now();
        let mut tab: Option<String> = None;
        let mut front_tab: Option<String> = None;
        let mut last_tab = Instant::now();
        let mut last_sig: Option<(Option<i64>, Option<i64>, Option<String>, Option<String>, bool, bool)> = None;
        let _ = tx.send(Event::Media(youtube.clone()));
        loop {
            match cmd_rx.recv_timeout(TICK) {
                Ok(cmd) => { if let MediaCmd::CardRect(rect) = cmd { card = rect; trace!("card {card:?}"); } else { handle(&mut player, &mut youtube, cmd, &socket, card) } },
                Err(RecvTimeoutError::Timeout) => if last_poll.elapsed() >= POLL { last_poll = Instant::now(); poll(&mut player, &mut youtube) },
                // The rail is gone: dropping the player stops mpv with it.
                Err(RecvTimeoutError::Disconnected) => break,
            }
            // The video window is always on top, so it may only exist while our own terminal is in
            // front. Polled about once a second and only while a player is alive: the check costs one
            // osascript call, which is cheap compared with an always-visible window over other apps.
            if let Some(p) = player.as_mut() {
                if watch.is_none() { watch = spawn_focus_watch() }
                if ours.is_none() { ours = terminal_pid() }
                let front = match &watch { Some((_, shared)) => match shared.load(Ordering::Relaxed) { -1 => frontmost_pid(), pid => Some(pid) }, None => frontmost_pid() };
                if last_tab.elapsed() >= Duration::from_millis(150) {
                    last_tab = Instant::now();
                    if tab.is_none() { tab = std::env::var("HERDR_PANE_ID").ok().and_then(|id| our_tab(&socket, &id)) }
                    front_tab = focused_tab(&socket);
                }
                let same_tab = match (&tab, &front_tab) { (Some(a), Some(b)) => a == b, _ => true };
                let raw = visible_now(front, ours, Some(p.child.id()), same_tab, card.is_some());
                let now = follow(&mut missing_ticks, raw);
                let sig = (front, ours, tab.clone(), front_tab.clone(), card.is_some(), now);
                if last_sig.as_ref() != Some(&sig) { last_sig = Some(sig); trace!("state player={} front={front:?} ours={ours:?} own_tab={tab:?} front_tab={front_tab:?} card={} raw={raw} now={now}", p.child.id(), card.is_some()); }
                let verify = last_verify.elapsed() >= VERIFY;
                if verify { last_verify = Instant::now(); }
                apply_visibility(p, &mut shown, now, verify);
                move_window(&mut player, &mut youtube, card, now, &mut moved_at, &socket);
            } else { shown = None; moved_at = None; missing_ticks = 0; if let Some((mut c, _)) = watch.take() { let _ = c.kill(); let _ = c.wait(); } }
            if youtube != sent { sent = youtube.clone(); if tx.send(Event::Media(youtube.clone())).is_err() { break } }
        }
    });
    cmd_tx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn video(id: &str) -> Video { Video { id: id.into(), title: id.into() } }

    #[test]
    fn movement_decision_distinguishes_position_and_size() {
        let a = CellRect::new(3, 20, 40, 10);
        let b = CellRect::new(3, 24, 40, 10);
        let c = CellRect::new(3, 24, 41, 10);
        assert_eq!(format!("{:?}", needs_move(Some(a), Some(a), true)), "None");
        assert_eq!(format!("{:?}", needs_move(Some(a), Some(b), true)), "Rebuild");
        assert_eq!(format!("{:?}", needs_move(Some(a), Some(c), true)), "Rebuild");
    }

    #[test]
    fn playlist_engine_contract() {
        assert_eq!(playlist_request("abc &"), ("https://www.youtube.com/playlist?list=abc%20%26".to_string(), 200));
        assert_eq!(parse_pairs("a\tAlpha\n\tMissing\nb\t \nno-tab\nc\tGamma\n"), vec![("a".into(), "Alpha".into()), ("c".into(), "Gamma".into())]);
        let mut y = Youtube::default();
        apply_playlists(&mut y, Ok(vec![("p".into(), "List".into())]));
        assert_eq!(y.state, "1 listas");
        assert!(y.playlist_search_done);
        apply_videos(&mut y, "p", Ok(vec![video("a"), video("b")]));
        assert_eq!(y.selected, Some(0));
        assert_eq!(y.state, "2 videos");
        assert_eq!(eof_decision(&y.videos, 0, true), EofDecision::Next);
        assert_eq!(eof_decision(&y.videos, 1, true), EofDecision::Stop);
        assert_eq!(eof_decision(&y.videos, 1, false), EofDecision::Refill);
        apply_playlists(&mut y, Ok(vec![]));
        assert_eq!(y.state, "sin listas");
        apply_playlists(&mut y, Err("falló".into()));
        assert_eq!(y.state, "falló");
        assert!(y.playlists.is_empty());
    }

    #[test]
    fn playlist_search_encodes_query_and_filters_to_playlists() {
        assert_eq!(playlists_url("música & jazz"), "https://www.youtube.com/results?search_query=m%C3%BAsica%20%26%20jazz&sp=EgIQAw%253D%253D");
    }

    #[test]
    fn layout_parser_selects_own_pane_and_rejects_missing_data() {
        let v = json!({"layout":{"area":{"x":0,"y":0,"width":200,"height":50},"panes":[{"pane_id":"hud","rect":{"x":150,"y":0,"width":50,"height":50}}]}});
        assert_eq!(parse_layout(&v, "hud"), Some((CellRect::new(0,0,200,50), CellRect::new(150,0,50,50))));
        assert_eq!(parse_layout(&v, "other"), None);
        assert_eq!(parse_layout(&json!({"layout":{"area":{},"panes":[]}}), "hud"), None);
    }

    #[test]
    fn pixel_geometry_scales_pane_origin_and_clamps() {
        let win = (10, 20, 1000, 500);
        let area = CellRect::new(0,0,100,50);
        let full = CellRect::new(0,0,100,50);
        let g = grid_of(win, area, full, None);
        assert_eq!(geometry(g, area, full, CellRect::new(10,5,20,10)).as_deref(), Some("200x100+110+70"));
        let odd = CellRect::new(0,0,10,10); let small = CellRect::new(5,0,5,10);
        assert_eq!(geometry(grid_of((0,0,101,101), odd, small, None), odd, small, CellRect::new(0,0,2,2)).as_deref(), Some("20x20+50+0"));
        let right = CellRect::new(90,40,10,10);
        assert_eq!(geometry(grid_of(win, area, right, None), area, right, CellRect::new(5,5,100,100)).as_deref(), Some("50x50+960+470"));
        let outside = CellRect::new(100,0,1,1);
        assert!(geometry(grid_of(win, area, outside, None), area, outside, CellRect::new(0,0,1,1)).is_none());
    }

    #[test]
    fn the_video_lands_on_the_cells_the_card_reserved() {
        // Measured on the rail's own terminal (Ghostty on macOS): a 1920x1055 window whose text grid
        // starts 38 px below its title bar and 4 px in, drawing 8x17 cells — a 239x59 grid. The layout
        // area is 209x58, so the multiplexer spends 30 columns on its spaces sidebar and one row on its
        // tab bar before the area's first cell. A ten-row picture at area row 21 belongs at grid rows
        // 22..32 — screen rows 437..607, 170 px tall — not the 182 px the frame scaling asked for.
        let frame = (0, 25, 1920, 1055);
        let area = CellRect::new(0, 0, 209, 58);
        let pane = CellRect::new(157, 0, 52, 58);
        let card = CellRect::new(0, 21, 39, 10);
        let cells = Cells { w: 8, h: 17, cols: 50, rows: 56 };
        let grid = grid_of(frame, area, pane, Some(cells));
        assert_eq!((grid.x, grid.y), (0, 63), "the grid starts below the title bar, not at the frame's corner");
        assert_eq!((grid.skip_x, grid.skip_y), (31, 1), "the area starts past the sidebar and the tab bar");
        assert_eq!((grid.inset_x, grid.inset_y), (1, 1), "the pane's own grid starts inside its border");
        assert_eq!(geometry(grid, area, pane, card).as_deref(), Some("312x170+1512+454"));
    }

    #[test]
    fn without_a_cell_size_the_frame_is_all_there_is() {
        // Every terminal answers the pixel size of its grid, but not all of them: when the answer is
        // empty this falls back to scaling the area into the whole window frame, which is what it did
        // before the grid was measured. It is wrong by the chrome, and visibly better than nothing.
        let frame = (10, 20, 1000, 500);
        let area = CellRect::new(0, 0, 100, 50);
        let pane = CellRect::new(0, 0, 100, 50);
        assert_eq!(geometry(grid_of(frame, area, pane, None), area, pane, CellRect::new(10, 5, 20, 10)).as_deref(), Some("200x100+110+70"));
    }

    #[test]
    fn a_negative_window_origin_is_not_formatted_with_a_double_sign() {
        // A display left of or above the main one gives a negative origin. mpv reads `-20` and rejects
        // `+-20`, so the sign must come from Rust's own formatting.
        let area = CellRect::new(0, 0, 100, 50);
        let pane = CellRect::new(0, 0, 100, 50);
        let card = CellRect::new(10, 20, 40, 10);
        let got = geometry(grid_of((-120, -25, 1000, 500), area, pane, None), area, pane, card).expect("geometry");
        assert_eq!(got, "400x100-20+175");
        assert!(!got.contains("+-"), "{got}");
    }

    #[test]
    fn missing_geometry_inputs_have_a_fallback_reason() {
        let r = CellRect::new(0,0,100,50);
        assert!(placement(None, Some((r,r)), Some(r)).is_err());
        assert!(placement(Some((0,0,100,100)), None, Some(r)).is_err());
        assert!(placement(Some((0,0,100,100)), Some((r,r)), None).is_err());
        assert!(placement(Some((0,0,100,100)), Some((r,r)), Some(r)).is_ok());
    }

    #[test]
    fn hiding_waits_for_the_block_to_stay_gone() {
        let mut missing = 0u8;
        assert!(follow(&mut missing, true), "a block on screen means the window is on screen");
        assert!(follow(&mut missing, false), "one frame without the block is a redraw, not the card leaving");
        assert!(follow(&mut missing, true), "the block came back");
        assert_eq!(missing, 0, "and the count resets");
        let mut last = true;
        for _ in 0..HIDE_TICKS { last = follow(&mut missing, false); }
        assert!(!last, "the block stayed gone, so the window goes with it");
        assert!(follow(&mut missing, true), "showing never waits");
    }

    #[test]
    fn a_block_that_moved_asks_for_a_new_player() {
        let a = CellRect::new(3, 20, 40, 10);
        let b = CellRect::new(3, 24, 40, 10);
        assert_eq!(needs_move(Some(a), Some(b), true), Movement::Rebuild);
        assert_eq!(needs_move(Some(a), Some(a), true), Movement::None);
        assert_eq!(needs_move(Some(a), Some(b), false), Movement::None);
        assert_eq!(needs_move(None, None, true), Movement::None);
        // A player is recorded for the block it was built for, whether or not the placement worked.
        // Recording where it landed, or nothing at all, asked for a new player on every tick: an endless
        // kill-and-respawn of mpv that stole the keyboard from the rail and left the sidebar frozen.
        assert_eq!(needs_move(Some(b), Some(b), true), Movement::None);
        assert_eq!(needs_move(Some(b), Some(b), false), Movement::None);
    }

    #[test]
    fn a_block_that_keeps_moving_spends_no_rebuild() {
        let a = CellRect::new(3, 20, 40, 10);
        let b = CellRect::new(3, 24, 40, 10);
        let t0 = Instant::now();
        let mut pending = None;
        // Still moving: every new place restarts the wait instead of spending it on a stale rebuild.
        assert!(!settled(&mut pending, a, t0));
        assert!(!settled(&mut pending, b, t0 + Duration::from_millis(200)));
        assert!(!settled(&mut pending, a, t0 + Duration::from_millis(400)));
        assert!(!settled(&mut pending, a, t0 + Duration::from_millis(500)));
        // It stopped: exactly one rebuild, and the next frame starts a fresh wait.
        assert!(settled(&mut pending, a, t0 + Duration::from_millis(801)));
        assert!(!settled(&mut pending, a, t0 + Duration::from_millis(900)));
    }

    #[test]
    fn the_focus_helper_line_is_a_pid_or_nothing() {
        assert_eq!(parse_pid("1034"), Some(1034));
        assert_eq!(parse_pid("1034\n"), Some(1034));
        assert_eq!(parse_pid("-1"), None, "the helper's starting value is not an app");
        assert_eq!(parse_pid(""), None);
        assert_eq!(parse_pid("No application"), None);
    }

    #[test]
    fn the_window_shows_only_while_our_terminal_is_in_front() {
        assert!(visible_now(Some(7), Some(7), None, true, true));
        assert!(!visible_now(Some(9), Some(7), None, true, true), "another app in front hides the video");
        assert!(visible_now(None, Some(7), None, true, true), "an unreadable frontmost app must not hide the video");
        assert!(visible_now(Some(7), None, None, true, true), "nor must an unknown terminal");
        assert!(visible_now(Some(9), Some(7), Some(9), true, true), "our own player taking focus is not another app");
        assert!(!visible_now(Some(9), Some(7), Some(8), true, true));
        assert!(!visible_now(Some(7), Some(7), None, false, true), "another herdr tab hides the video");
        assert!(!visible_now(Some(7), Some(7), None, true, false), "another tab of the rail hides the video");
        assert!(visible_now(Some(7), Some(7), None, true, true));
    }

    #[test]
    fn the_window_visibility_command_is_a_minimize_toggle() {
        // Measured on macOS: `window-minimized` removes the always-on-top window from the screen and
        // restores it at the same rectangle, which is what "hide while another app is in front" needs.
        assert!(!window_value(true), "visible means not minimized");
        assert!(window_value(false));
    }

    #[test]
    fn mpv_arguments_keep_playback_flags_with_optional_geometry() {
        let args = mpv_args("/tmp/mpv.sock", None);
        assert_eq!(args, vec!["--idle=yes", "--no-terminal", "--msg-level=all=error", "--keep-open=yes", "--title=gs3 media", "--input-ipc-server=/tmp/mpv.sock", "--border=no", "--osc=no", "--ontop", "--panscan=1.0", "--macos-app-activation-policy=accessory"]);
        // The geometry we compute is in screen coordinates, so mpv must treat it as such: with the
        // default `visible` calculation it shifts the window down by the menu bar (measured: 25 px).
        let mut expected = args; expected.push("--geometry=100x50+1+2".into()); expected.push("--macos-geometry-calculation=whole".into()); expected.push("--force-window=yes".into());
        assert_eq!(mpv_args("/tmp/mpv.sock", Some("100x50+1+2")), expected);
        assert!(!mpv_args("/tmp/mpv.sock", None).iter().any(|a| a.starts_with("--macos-geometry")));
        // S11: no black bands. The card's block is only approximately 16:9, so whatever is left after
        // the window is sized to it gets cropped instead of drawn as letterbox.
        assert!(mpv_args("/tmp/mpv.sock", None).contains(&"--panscan=1.0".to_string()));
        // The window must exist before the file loads, or mpv creates it later and applies the
        // geometry off by the menu bar (measured: asked +1488+406, landed at +1513+438).
        assert!(mpv_args("/tmp/mpv.sock", Some("1x1+1+1")).contains(&"--force-window=yes".to_string()));
        // An accessory app never becomes active: the player would otherwise take focus from the
        // terminal every time a video starts, and the focus rule would hide it immediately.
        assert!(mpv_args("/tmp/mpv.sock", None).contains(&"--macos-app-activation-policy=accessory".to_string()));
        assert!(!mpv_args("/tmp/mpv.sock", None).contains(&"--force-window=yes".to_string()));
    }

    #[test]
    fn window_rect_parser_reads_coregraphics_bounds_and_rejects_the_rest() {
        // The shape CoreGraphics returns, read without any Accessibility or Automation grant: only a
        // window's title is protected, its geometry is not.
        assert_eq!(parse_window_json(r#"{"X":0,"Height":1055,"Y":25,"Width":1920}"#), Some((0, 25, 1920, 1055)));
        assert_eq!(parse_window_json(r#"{"X":-1920,"Height":1000,"Y":-30,"Width":1920}"#), Some((-1920, -30, 1920, 1000)));
        assert_eq!(parse_window_json("null"), None, "no window of ours is on screen");
        assert_eq!(parse_window_json(r#"{"X":0,"Y":25,"Width":0,"Height":1055}"#), None);
        assert_eq!(parse_window_json(r#"{"X":0,"Y":25}"#), None);
        assert_eq!(parse_window_json("garbage"), None);
        assert_eq!(parse_window_json(""), None);
    }

    #[test]
    fn eof_guard_fires_once_and_rearms_only_for_a_new_load() {
        let mut guard = EofGuard::Waiting;
        assert!(!guard.observe(true));
        assert!(!guard.observe(false));
        assert!(guard.observe(true));
        assert!(!guard.observe(true));
        assert!(!guard.observe(false));
        assert!(!guard.observe(true));
        guard = EofGuard::Waiting;
        assert!(!guard.observe(false));
        assert!(guard.observe(true));
    }

    #[test]
    fn next_recommendation_or_refill_skips_the_finished_id() {
        let list = vec![video("a"), video("b"), video("c")];
        assert_eq!(next_index(&list, 0), Some(1));
        assert_eq!(next_index(&list, 2), None);
        assert_eq!(refill_index(&[video("c"), video("c"), video("d")], "c"), Some(2));
        assert_eq!(refill_index(&[video("c")], "c"), None);
        assert_eq!(refill_index(&[], "c"), None);
    }

    #[test]
    fn tools_are_found_even_when_the_inherited_path_is_minimal() {
        // `sh` exists on every runner. A launcher may hand the rail a PATH without Homebrew.
        assert!(find_tool("sh").is_some());
        assert!(find_tool("gs3-hud-no-such-tool-exists").is_none());
        let path = tool_path();
        assert!(path.split(':').any(|d| d == "/opt/homebrew/bin" || d == "/usr/local/bin"), "{path}");
    }

    /// The queue is the seed first, then the recommendations YouTube itself returns for it.
    #[test]
    #[ignore = "needs network access and yt-dlp; run with: cargo test -- --ignored"]
    fn live_queue_starts_with_the_seed_and_continues_with_recommendations() {
        let list = queue("daft punk around the world").expect("yt-dlp answered");
        assert!(list.len() > 1, "a mix must carry recommendations, got {}", list.len());
        assert!(list.iter().all(|v| !v.id.is_empty() && !v.title.is_empty()));
        assert_ne!(list[0].id, list[1].id, "the seed and the first recommendation differ");
    }
}
