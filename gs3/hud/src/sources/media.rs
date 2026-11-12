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

fn geometry(win: (i64,i64,i64,i64), area: CellRect, pane: CellRect, card: CellRect) -> Option<String> {
    if win.2 <= 0 || win.3 <= 0 || area.width == 0 || area.height == 0 || card.width == 0 || card.height == 0 { return None }
    fn axis(origin: i64, size: i64, grid_start: u32, grid_size: u32, pane_start: u32, pane_size: u32, start: u32, length: u32) -> Option<(i64,i64)> {
        let grid_start = i128::from(grid_start); let grid_end = grid_start + i128::from(grid_size);
        let pane_start = i128::from(pane_start); let pane_end = pane_start + i128::from(pane_size);
        let begin = (pane_start + i128::from(start)).max(grid_start);
        let end = (pane_start + i128::from(start) + i128::from(length)).min(grid_end).min(pane_end);
        if begin >= end { return None }
        let scale = |v: i128| (v - grid_start) * i128::from(size) / i128::from(grid_size);
        let a = scale(begin); let b = scale(end);
        if b <= a { return None }
        Some((i64::try_from(i128::from(origin) + a).ok()?, i64::try_from(b-a).ok()?))
    }
    let (x,w) = axis(win.0,win.2,area.x,area.width,pane.x,pane.width,card.x,card.width)?;
    let (y,h) = axis(win.1,win.3,area.y,area.height,pane.y,pane.height,card.y,card.height)?;
    // Rust's own sign: mpv accepts `-20` and rejects `+-20`.
    Some(format!("{w}x{h}{x:+}{y:+}"))
}

fn placement(window: Option<(i64,i64,i64,i64)>, layout: Option<(CellRect,CellRect)>, card: Option<CellRect>) -> Result<String, &'static str> {
    let card = card.ok_or("sin área visible de YouTube")?;
    let (area,pane) = layout.ok_or("sin ubicación del panel")?;
    let window = window.ok_or("sin ubicación de ventana del terminal")?;
    geometry(window, area, pane, card).ok_or("área de video inválida")
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

/// The window cannot be moved at runtime, so a block that changed while the video is on screen means a
/// new player at the same video and position. `used` is the block the running player was started for,
/// not where it landed: a player that was built while the card sat at `want` needs no further rebuild,
/// even if the placement itself failed. `want` is where the card is now.
fn needs_move(used: Option<CellRect>, want: Option<CellRect>, visible: bool) -> bool {
    visible && want.is_some() && used != want
}

/// Rebuild the player on the same video and position when the block moved, because mpv cannot be
/// moved once its window exists (measured: `set_property geometry` answers success and does nothing).
/// Debounced: a scroll in progress would otherwise restart the video on every tick.
fn move_window(player: &mut Option<Player>, y: &mut Youtube, card: Option<CellRect>, visible: bool, moved_at: &mut Option<Instant>, socket: &std::path::Path) {
    let Some(p) = player.as_mut() else { *moved_at = None; return };
    if !needs_move(p.geom, card, visible) { *moved_at = None; return }
    let since = moved_at.get_or_insert_with(Instant::now);
    if since.elapsed() < Duration::from_millis(300) { return }
    *moved_at = None;
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

fn handle(player: &mut Option<Player>, y: &mut Youtube, cmd: MediaCmd, socket: &std::path::Path) {
    if let MediaCmd::Play(query, card) = &cmd {
        trace!("play query={query} card={card:?} player_exists={}", player.is_some());
        if let Some(player) = player.as_mut() { player.eof = EofGuard::Waiting; }
        y.state = format!("buscando {query}");
        match queue(query) {
            Ok(list) if !list.is_empty() => {
                let mut fallback = None;
                if player.is_none() {
                    let layout = std::env::var("HERDR_PANE_ID").ok().and_then(|id| super::request(socket, "pane.layout", json!({"pane_id":id})).ok().and_then(|v| parse_layout(&v, &id)));
                    let result = placement(window_rect(), layout, *card);
                    fallback = result.as_ref().err().copied();
                    trace!("place at play: geometry={:?} fallback={fallback:?}", result.as_deref().ok());
                    *player = Player::start(result.as_deref().ok(), *card);
                    trace!("player at play: {:?}", player.as_ref().map(|p| p.child.id()));
                }
                let Some(player) = player.as_mut() else { y.state = "mpv no encontrado".into(); return };
                y.queue = list; y.index = 0; y.state = fallback.map(|reason| format!("sin geometría: {reason}")).unwrap_or_else(|| "reproduciendo".into());
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
        MediaCmd::Play(..) => unreachable!("handled above"),
    }
}

fn poll(player: &mut Option<Player>, y: &mut Youtube) {
    let Some(player) = player.as_mut() else { return };
    if let Some(v) = player.get("duration").and_then(|v| v.as_f64()) { y.duration = v }
    if let Some(v) = player.get("time-pos").and_then(|v| v.as_f64()) { y.position = v.max(0.0) }
    if let Some(v) = player.get("pause").and_then(|v| v.as_bool()) { y.playing = !v }
    if let Some(v) = player.get("media-title").and_then(|v| v.as_str().map(str::to_owned)) { if !v.is_empty() { y.title = Some(v) } }
    let Some(eof) = player.get("eof-reached").and_then(|v| v.as_bool()) else { return };
    if !player.eof.observe(eof) { return }
    if let Some(finished) = y.queue.get(y.index).cloned() {
        if next_index(&y.queue, y.index).is_some() {
            advance(player, y, 1);
            return;
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
        let mut moved_at: Option<Instant> = None;
        let mut missing_ticks: u8 = 0;
        let mut last_verify = Instant::now();
        let mut tab: Option<String> = None;
        let mut front_tab: Option<String> = None;
        let mut last_tab = Instant::now();
        let mut last_sig: Option<(Option<i64>, Option<i64>, Option<String>, Option<String>, bool, bool)> = None;
        let _ = tx.send(Event::Media(youtube.clone()));
        loop {
            match cmd_rx.recv_timeout(TICK) {
                Ok(cmd) => { if let MediaCmd::CardRect(rect) = cmd { card = rect; trace!("card {card:?}"); } else { handle(&mut player, &mut youtube, cmd, &socket) } },
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
        assert_eq!(geometry(win, area, CellRect::new(0,0,100,50), CellRect::new(10,5,20,10)).as_deref(), Some("200x100+110+70"));
        assert_eq!(geometry((0,0,101,101), CellRect::new(0,0,10,10), CellRect::new(5,0,5,10), CellRect::new(0,0,2,2)).as_deref(), Some("20x20+50+0"));
        assert_eq!(geometry(win, area, CellRect::new(90,40,10,10), CellRect::new(5,5,100,100)).as_deref(), Some("50x50+960+470"));
        assert!(geometry(win, area, CellRect::new(100,0,1,1), CellRect::new(0,0,1,1)).is_none());
    }

    #[test]
    fn a_negative_window_origin_is_not_formatted_with_a_double_sign() {
        // A display left of or above the main one gives a negative origin. mpv reads `-20` and rejects
        // `+-20`, so the sign must come from Rust's own formatting.
        let area = CellRect::new(0, 0, 100, 50);
        let pane = CellRect::new(0, 0, 100, 50);
        let card = CellRect::new(10, 20, 40, 10);
        let got = geometry((-120, -25, 1000, 500), area, pane, card).expect("geometry");
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
        assert!(needs_move(Some(a), Some(b), true), "the card scrolled and the window cannot follow");
        assert!(!needs_move(Some(a), Some(a), true), "same block, nothing to do");
        assert!(!needs_move(Some(a), Some(b), false), "a hidden window does not need to move yet");
        assert!(!needs_move(None, None, true));
        // A player is recorded for the block it was built for, whether or not the placement worked.
        // Recording where it landed, or nothing at all, asked for a new player on every tick: an endless
        // kill-and-respawn of mpv that stole the keyboard from the rail and left the sidebar frozen.
        assert!(!needs_move(Some(b), Some(b), true), "the rebuild already followed this block");
        assert!(!needs_move(Some(b), Some(b), false));
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
