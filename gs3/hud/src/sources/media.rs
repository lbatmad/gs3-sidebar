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
use crate::media::{MediaCmd, Video, Youtube};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

const POLL: Duration = Duration::from_millis(500);
const QUEUE_MAX: usize = 24;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// A Homebrew installation is not always on the PATH the rail inherits from its launcher.
fn find_tool(name: &str) -> Option<PathBuf> {
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
    fn start() -> Option<Player> {
        let mpv = find_tool("mpv")?;
        let sock = std::env::temp_dir().join(format!("gs3-hud-{}-mpv.sock", std::process::id()));
        let _ = std::fs::remove_file(&sock);
        let child = Command::new(mpv)
            // Keep EOF observable until the engine has polled and loaded the next video.
            .args(["--idle=yes", "--no-terminal", "--msg-level=all=error", "--keep-open=yes", "--title=gs3 media"])
            .arg(format!("--input-ipc-server={}", sock.display()))
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

fn handle(player: &mut Option<Player>, y: &mut Youtube, cmd: MediaCmd) {
    if let MediaCmd::Play(query) = &cmd {
        if let Some(player) = player.as_mut() { player.eof = EofGuard::Waiting; }
        y.state = format!("buscando {query}");
        match queue(query) {
            Ok(list) if !list.is_empty() => {
                if player.is_none() { *player = Player::start(); }
                let Some(player) = player.as_mut() else { y.state = "mpv no encontrado".into(); return };
                y.queue = list; y.index = 0; y.state = "reproduciendo".into();
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
    let Some(player) = player.as_mut() else { y.state = "mpv no encontrado".into(); return };
    match cmd {
        MediaCmd::Toggle => { player.call(&["cycle", "pause"]); }
        MediaCmd::Next => advance(player, y, 1),
        MediaCmd::Prev => advance(player, y, -1),
        MediaCmd::Seek(fraction) => {
            if y.duration > 0.0 { player.call(&["seek", &format!("{:.3}", fraction.clamp(0.0, 1.0) * y.duration), "absolute"]); }
        }
        MediaCmd::Play(_) => unreachable!("handled above"),
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
pub fn media(tx: Sender<Event>) -> Sender<MediaCmd> {
    let (cmd_tx, cmd_rx) = mpsc::channel();
    thread::spawn(move || {
        let mut youtube = Youtube { state: if find_tool("mpv").is_some() { "sin reproducción".into() } else { "mpv no encontrado".into() }, ..Youtube::default() };
        let mut player: Option<Player> = None;
        let mut sent = youtube.clone();
        let _ = tx.send(Event::Media(youtube.clone()));
        loop {
            match cmd_rx.recv_timeout(POLL) {
                Ok(cmd) => handle(&mut player, &mut youtube, cmd),
                Err(RecvTimeoutError::Timeout) => poll(&mut player, &mut youtube),
                // The rail is gone: dropping the player stops mpv with it.
                Err(RecvTimeoutError::Disconnected) => break,
            }
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
