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
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(500);
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
fn visible_now(frontmost: Option<i64>, ours: Option<i64>) -> bool {
    match (frontmost, ours) { (Some(f), Some(o)) => f == o, _ => true }
}

/// The app in front, from the window list CoreGraphics orders front to back. No permission needed.
fn frontmost_pid() -> Option<i64> {
    let script = r#"ObjC.import("CoreGraphics");
const list = ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo($.kCGWindowListOptionOnScreenOnly, 0)));
const hit = list.find(w => (w.kCGWindowLayer|0) === 0 && (w.kCGWindowBounds.Width|0) > 100);
hit ? String(hit.kCGWindowOwnerPID|0) : "";"#;
    let out = Command::new("osascript").args(["-l", "JavaScript", "-e", script]).output().ok()?;
    String::from_utf8_lossy(&out.stdout).trim().parse::<i64>().ok()
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

/// The value mpv's `window-minimized` takes for the requested visibility.
fn window_arg(visible: bool) -> &'static str { if visible { "false" } else { "true" } }

fn mpv_args(socket: &str, geometry: Option<&str>) -> Vec<String> {
    let mut args = vec!["--idle=yes".into(), "--no-terminal".into(), "--msg-level=all=error".into(), "--keep-open=yes".into(), "--title=gs3 media".into(), format!("--input-ipc-server={socket}"), "--border=no".into(), "--osc=no".into(), "--ontop".into(), "--panscan=1.0".into()];
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
struct Player { child: Child, out: UnixStream, inp: BufReader<UnixStream>, id: u64, eof: EofGuard }

impl Player {
    fn start(geometry: Option<&str>) -> Option<Player> {
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
        Some(Player { child, out, inp, id: 0, eof: EofGuard::default() })
    }

    /// Send one JSON IPC command and wait for the reply carrying its `request_id`.
    /// Replies interleave with mpv's own events, so a plain read is not enough.
    fn call(&mut self, command: &[&str]) -> Option<Value> {
        self.id += 1;
        let id = self.id;
        self.out.write_all(json!({"command": command, "request_id": id}).to_string().as_bytes()).ok()?;
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
        if let Some(player) = player.as_mut() { player.eof = EofGuard::Waiting; }
        y.state = format!("buscando {query}");
        match queue(query) {
            Ok(list) if !list.is_empty() => {
                let mut fallback = None;
                if player.is_none() {
                    let layout = std::env::var("HERDR_PANE_ID").ok().and_then(|id| super::request(socket, "pane.layout", json!({"pane_id":id})).ok().and_then(|v| parse_layout(&v, &id)));
                    let result = placement(window_rect(), layout, *card);
                    fallback = result.as_ref().err().copied();
                    *player = Player::start(result.as_deref().ok());
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
    if let MediaCmd::Window(visible) = cmd {
        // A player that is not there yet has nothing to hide, and must not report an error for it.
        if let Some(player) = player.as_mut() { player.call(&["set_property", "window-minimized", window_arg(visible)]); }
        return;
    }
    let Some(player) = player.as_mut() else { y.state = "mpv no encontrado".into(); return };
    match cmd {
        MediaCmd::Window(_) => unreachable!("handled above"),
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
        let mut shown: Option<bool> = None;
        let mut last_focus = Instant::now();
        let _ = tx.send(Event::Media(youtube.clone()));
        loop {
            match cmd_rx.recv_timeout(POLL) {
                Ok(cmd) => handle(&mut player, &mut youtube, cmd, &socket),
                Err(RecvTimeoutError::Timeout) => poll(&mut player, &mut youtube),
                // The rail is gone: dropping the player stops mpv with it.
                Err(RecvTimeoutError::Disconnected) => break,
            }
            // The video window is always on top, so it may only exist while our own terminal is in
            // front. Polled about once a second and only while a player is alive: the check costs one
            // osascript call, which is cheap compared with an always-visible window over other apps.
            if let Some(p) = player.as_mut() {
                if last_focus.elapsed() >= Duration::from_millis(1200) {
                    last_focus = Instant::now();
                    if ours.is_none() { ours = terminal_pid() }
                    let now = visible_now(frontmost_pid(), ours);
                    if shown != Some(now) { shown = Some(now); p.call(&["set_property", "window-minimized", window_arg(now)]); }
                }
            } else { shown = None }
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
    fn the_window_shows_only_while_our_terminal_is_in_front() {
        assert!(visible_now(Some(7), Some(7)));
        assert!(!visible_now(Some(9), Some(7)), "another app in front hides the video");
        assert!(visible_now(None, Some(7)), "an unreadable frontmost app must not hide the video");
        assert!(visible_now(Some(7), None), "nor must an unknown terminal");
    }

    #[test]
    fn the_window_visibility_command_is_a_minimize_toggle() {
        // Measured on macOS: `window-minimized` removes the always-on-top window from the screen and
        // restores it at the same rectangle, which is what "hide while another app is in front" needs.
        assert_eq!(window_arg(true), "false");
        assert_eq!(window_arg(false), "true");
    }

    #[test]
    fn mpv_arguments_keep_playback_flags_with_optional_geometry() {
        let args = mpv_args("/tmp/mpv.sock", None);
        assert_eq!(args, vec!["--idle=yes", "--no-terminal", "--msg-level=all=error", "--keep-open=yes", "--title=gs3 media", "--input-ipc-server=/tmp/mpv.sock", "--border=no", "--osc=no", "--ontop", "--panscan=1.0"]);
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
