mod config;
mod cover;
mod core;
mod media;
mod sources;
mod syntax;
mod ui;
use anyhow::{anyhow, Result};
use core::*;
use serde_json::Value;
use crossterm::{event::{self, Event as Input, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind}, execute, style::Print, terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen}};
use ratatui::{backend::CrosstermBackend, style::{Color, Modifier, Style}, text::{Line, Span}, widgets::Paragraph, Terminal};
use std::{io::{self, Write}, path::{Path, PathBuf}, sync::mpsc, thread};

const HELP:&str="gs3-hud --pane <HERDR_PANE_ID> [--socket <path>] [--cwd <dir>] [--config <file>] [--once]\n\nRead-only Pi telemetry rail (tabs MAIN GIT PROF MEM TREE MEDIA and ⛭ = CONFIG).\nKeys: 1-7 switch tab (in that order), r refresh, q / Ctrl-C quit (in the CONFIG name editor: type, Backspace, Enter saves, Esc cancels).\nMouse: click a tab, a card's [-]/[+] to collapse or expand it, CONFIG rows to toggle tabs or edit the name;\nGIT: [ Grafo | Ramas ] switches view, a branch shows its diff against HEAD ([ ✕ ] closes), a file previews its diff inline (read-only git diff), and a commit opens the rail's own window with its whole message — author, when and the body — above the files it touched, each row clickable to that file's diff; the graph shows 25 rows at a time (the `↕ 1-25/30` row says which slice) and the wheel over the card walks back through the history, asking for the next page — up to 300 commits — when the window reaches the end of what was fetched;\nMAIN: controls use Nerd Font icons by default; plain_controls=true in --config selects ASCII MIC/MUTE, HP/DEAF, APP and CAP labels for ordinary fonts. Mic / headphones toggle voice mute/deafen (Vesktop on Linux, official Discord on macOS; coral active, mint inactive, dim offline); the Discord icon opens or focuses that client; the capture control opens the area selector (Flameshot on Linux, native macOS selection to clipboard; explicit click, ≥1 s apart); an explicit voice click may request one-time consent and save a private 0600 token in ~/.config/gs3/discord-token.json (never displayed). No voice sockets in --once.\nMAIN: click any model/effort row (or Status ▸) to focus the bound agent pane and send fixed F9 for its model picker; repeated clicks within 1 s are ignored, failures are silent;\nPROF: reads global configured assignments from the canonical /gentle:profiles models.json; GENTLE_PI_CONFIG_HOME or absolute GENTLE_SHELL_CONFIG_HOME selects the store, then a bundled <prefix>/agent next to <prefix>/config/models.json, then the standard ~/.pi/gentle-ai store. The host row keeps the settings.json defaults until the profile configures an orchestrator entry. Provider roles are included without MD files; missing/invalid/unknown sources are labeled, never replaced by task/MD model guesses. The Task Manager block counts the subagent task history in the agent dir (done, failed, running) and names every task alive now: the directory is watched, so a task appears while it runs instead of after the next `r`. Above the roster, a Perfiles card lists every profile of the profiles.json store when there is more than one (role count and host model; the store's active marked with ●; the store file is watched with a 300 ms debounce). A click on a profile with a sendable name loads it through the session channel: the profile-server extension of the agent pane listens on /tmp/gs3-profile-<uid>-<pane>.sock (0600), the click sends one JSON `use` request with the name as data — nothing is ever typed into the agent pane — throttled to one per second, binding the profile to the current session only (no repo declaration, no pins, no global routing are touched) and switching the live model when the profile has an orchestrator route; the reply rewrites Profiles & Clusters with the loaded profile's model map (`perfil de sesión`), the loaded row carries ◉ beside the store's ● and the repo pin's ▣, and `r` re-asks the channel (a missing extension is shown as `canal no disponible`, never an error). An unsendable stored name keeps the visual-only selection. The `▸ /gentle:profiles · abrir` footer row focuses the bound agent pane and sends the literal command — text + Enter through the herdr API, nothing interpreted or simulated in the rail — one command per explicit click, repeats within 1 s discarded. r re-reads. Session pins/snapshots may differ; profiles and model selections are never written.\nTREE: a folder opens/closes, [ ⊟ ] folds all, a changed file opens its diff in GIT, and any other file opens in the rail's own window (gs3-panel, the same window a commit uses) with its text, rendered as a document if it is markdown (.md), coloured by its grammar if it is source code (the Sublime grammars that ship with the binary), or the image itself; a symlink, a sensitive name, a binary or anything outside the project is never shown and the window says why, and more than 256 KiB of text is shown cut;\nwheel over the tab bar cycles tabs, over a card's own long list (MEDIA `⌁ Pistas`) scrolls that list, elsewhere scrolls the body.\nMEDIA: Spotify keeps the upper section and YouTube the section below it. Spotify authorizes with its own PKCE consent: an explicit click opens the browser, waits up to 180 s on http://127.0.0.1:8899/callback (register that exact redirect in the app) and keeps client_id plus tokens in a private 0600 ~/.config/gs3/spotify.json through curl; nothing is fetched until you click. With Spotify Premium, start the gs3 librespot device explicitly, open its first-login OAuth URL shown on the card, then play the open playlist or a track, pause, skip forward or back and seek by clicking the progress bar, and set the volume, shuffle or repeat from the rows below the transport, which show what /me/player reports; `🔍 buscar tema` searches the catalogue and plays what you pick; the cover of the playing album (else of the open playlist) is drawn through the terminal's kitty graphics protocol, with no cover and no reserved space when the terminal or the fetch cannot provide it; the private ~/.config/gs3/librespot cache is reused. A YouTube search queues the best match plus its recommendations (the RD mix) and starts it; `listas` searches public playlists by name, lists a chosen playlist's videos in a ten-row window and a click starts there and continues to the end of that list, with no cookies; the video plays in a separate mpv window, never inside the rail: borderless, without on-screen controls and always on top, placed over the card below search from the window bounds CoreGraphics reports, which needs no permission. Otherwise playback starts without positioning and the card reports why. It hides whenever another application comes forward and returns when the rail does. A borderless window has no title bar to grab; there is no clean API to freeze another process's window. Transport and a clickable seek bar drive mpv over its JSON IPC socket. Requires librespot, mpv and yt-dlp (PATH, then /opt/homebrew/bin and /usr/local/bin); a missing tool is reported, never installed.\n--config is the HUD's own settings file {name, hidden, collapsed, optional plain_controls}: written atomically (in addition to the private Discord token and Spotify credential file, both 0600, and the private librespot cache); without it changes last until exit.\n--socket defaults to HERDR_SOCKET_PATH or ~/.config/herdr/herdr.sock\n--cwd defaults to the current directory\n--once prints a plain-text snapshot of every visible tab at width 42 (no terminal required)\nStack versions follow the selected installation; r refreshes them. Unverifiable tools show no verificada.\nPR refresh cycles (view + open count) are spaced by at least 30 seconds.";
struct Args {pane:String,socket:PathBuf,cwd:PathBuf,config:Option<PathBuf>,once:bool}
/// The channel socket path is keyed by the owner's uid: `id -u` is read once and cached.
fn uid()->String {use std::sync::OnceLock;static U:OnceLock<String>=OnceLock::new();U.get_or_init(||std::process::Command::new("id").arg("-u").output().ok().and_then(|o|String::from_utf8(o.stdout).ok()).map(|s|s.trim().to_string()).unwrap_or_default()).clone()}
fn args()->Result<Option<Args>> {
    let mut a=std::env::args().skip(1); let mut pane=None;
    let mut socket=std::env::var_os("HERDR_SOCKET_PATH").map(PathBuf::from).unwrap_or_else(||dirs::home_dir().unwrap_or_default().join(".config/herdr/herdr.sock"));
    let mut cwd=std::env::current_dir()?; let mut once=false; let mut config=None;
    while let Some(key)=a.next() {
        match key.as_str() {
            "--help"|"-h"=>{println!("{HELP}");return Ok(None)}
            "--once"=>once=true,
            "--pane"|"--socket"|"--cwd"|"--config"=>{let v=a.next().ok_or_else(||anyhow!("{key} requires a value"))?;match key.as_str(){"--pane"=>pane=Some(v),"--socket"=>socket=v.into(),"--config"=>config=Some(PathBuf::from(v)),_=>cwd=PathBuf::from(v).canonicalize()?}}
            _=>return Err(anyhow!("unknown option: {key}\n{HELP}"))
        }
    }
    Ok(Some(Args{pane:pane.filter(|s|!s.is_empty()).ok_or_else(||anyhow!("--pane is required\n{HELP}"))?,socket,cwd,config,once}))
}
// Button-only mouse reporting (1000) in SGR form (1006): clicks and wheel, never motion (1003).
const MOUSE_ON:&str="\x1b[?1000h\x1b[?1006h"; const MOUSE_OFF:&str="\x1b[?1006l\x1b[?1000l";
// Focus reporting: the video window is always on top, so it must vanish when another app comes forward.
const FOCUS_ON:&str="\x1b[?1004h"; const FOCUS_OFF:&str="\x1b[?1004l";
const COVER_DELETE: &str = "\x1b_Ga=d,d=I,i=731951,q=2\x1b\\";
const COVER_UNPLACE: &str = "\x1b_Ga=d,d=i,i=731951,q=2\x1b\\";
fn restore() {let _=disable_raw_mode(); let _=execute!(io::stdout(),Print(COVER_DELETE),Print(MOUSE_OFF),Print(FOCUS_OFF),LeaveAlternateScreen);}
struct Guard;
impl Drop for Guard {fn drop(&mut self){restore();}}
enum Message {Data(Event),Input(Input)}
fn main()->Result<()> {
    let Some(a)=args()? else {return Ok(())};
    let mut state=State::new(a.pane.clone(),a.cwd.clone());
    state.ui=ui::Ui::new(a.config.as_deref().map(config::load).unwrap_or_default(),a.config.clone());
    if a.once {
        match sources::pane(&a.socket,&a.pane) {
            Ok(Some(p))=>{apply(&mut state,Event::Connection(true));apply(&mut state,Event::Pane(p));}
            Ok(None)=>return Ok(()),
            Err(e)=>eprintln!("herdr: {e}"),
        }
        if let Some(path)=&state.session_path {let mut tail=Tail::default(); if let Err(e)=sources::read_tail(&PathBuf::from(path),&mut tail) {eprintln!("session: {e}");} state.session=tail.session;}
        state.git=sources::git_snapshot(&a.cwd); state.engram=sources::engram(&a.cwd); state.pr=sources::pr_once(&a.cwd);
        state.mcp=state.session_path.as_deref().and_then(|p|sources::mcp(Path::new(p)));
        state.roster=state.session_path.as_deref().and_then(|p|sources::roster_of(Path::new(p),&a.cwd)); state.stack=sources::stack(&a.cwd,state.session_path.as_deref().map(Path::new)); state.tree=Some(sources::tree(&a.cwd,Vec::new()));
        for tab in state.ui.tabs() {state.ui.tab=tab; for line in ui::render_lines(&state,42) {println!("{line}");}} return Ok(())
    }
    let previous=std::panic::take_hook(); std::panic::set_hook(Box::new(move|info|{restore();previous(info);}));
    let _guard=Guard;
    enable_raw_mode()?; execute!(io::stdout(),EnterAlternateScreen,Print(MOUSE_ON),Print(FOCUS_ON))?;
    let mut terminal=Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let (tx,rx)=mpsc::channel(); let input_tx=tx.clone();
    thread::spawn(move||{while let Ok(e)=event::read() {if input_tx.send(Message::Input(e)).is_err(){break}}});
    let (data_tx,data_rx)=mpsc::channel(); let bridge=tx.clone();
    thread::spawn(move||{for e in data_rx {if bridge.send(Message::Data(e)).is_err(){break}}});
    let local=sources::local(a.cwd.clone(),a.socket.clone(),a.pane.clone(),data_tx.clone()); let details=sources::details(a.cwd.clone(),data_tx.clone()); let pr=sources::prs(a.cwd.clone(),data_tx.clone());
    // GIT inspector: ask for a branch/file diff when the selection changes, and again when the repo changes under it.
    let request=|s:&State|{let u=&s.ui; (u.git_branch.is_some()||u.git_file.is_some()||u.git_commit.is_some()).then(||sources::DetailRequest{branch:u.git_branch.clone(),file:u.git_file.clone(),commit:u.git_commit.clone(),
        untracked:u.git_branch.is_none() && u.git_commit.is_none() && s.git.files.iter().any(|f|Some(&f.path)==u.git_file.as_ref() && f.kind==FileKind::Untracked)})};
    let mut asked:Option<sources::DetailRequest>=None;
    // TREE: scan only while the tab is shown — again when folders open/close, on `r`, and when the repo changes.
    let trees=sources::trees(a.cwd.clone(),data_tx.clone()); let mut scanned:Option<Vec<String>>=None;
    // TREE: the picked file is read off the draw loop, and only the newest click is answered.
    let previews=sources::previews(a.cwd.clone(),data_tx.clone()); let mut asked_preview:Option<String>=None;
    // Stack versions are resolved by the local worker on session binding and refresh.
    let picker=sources::model_picker(a.socket.clone(),a.pane.clone());
    // PROF: the `/gentle:profiles` screen opens on the bound pane, one command per explicit click.
    let profiles=sources::profiles_command(a.socket.clone(),a.pane.clone());
    // PROF: profile clicks load the profile through the session channel (no text reaches
    // the agent pane); the reply updates the cards as an Event::ProfileChannel.
    let profile_socket=sources::profile_channel_socket(&a.pane,uid().parse().unwrap_or(0));
    sources::profile_channel_worker(profile_socket.clone(),data_tx.clone());
    // The channel's current binding joins startup and every refresh.
    let profile_get=profile_socket.clone(); let profile_tx=data_tx.clone();
    let ask_profile=move || {let tx=profile_tx.clone(); let socket=profile_get.clone(); std::thread::spawn(move||{
        let _=tx.send(match sources::profile_channel_get(&socket) {
            Ok(Some(p))=>crate::core::Event::ProfileChannel(p),
            // Up with no binding: an explicit empty answer, not a down channel.
            Ok(None)=>crate::core::Event::ProfileChannel(Value::Bool(false)),
            Err(_)=>crate::core::Event::ProfileChannelDown,
        });
    });};
    let voice=sources::voice(data_tx.clone());
    let media=sources::media(data_tx.clone(),a.socket.clone()); let spotify=sources::spotify(data_tx.clone());
    let capture=sources::screenshot();
    let agent_pane=a.pane.clone(); let panel=sources::panel(a.socket.clone(),agent_pane.clone(),data_tx.clone()); let mut shown:Option<(String,usize)>=None;
    // The page of the graph already asked for: one request per limit, reset by the answer.
    let mut asked_page:Option<usize>=None;
    sources::herdr(a.socket.clone(),a.pane.clone(),data_tx);
    let mut draw=true; let mut last:Option<ui::Frame>=None;
    let mut transmitted: Option<String> = None;
    let mut sent_card: Option<Option<crate::media::CellRect>> = None;
    loop {
        if let Some(cmd)=state.ui.voice_cmd.take() {let _=voice.send(cmd);}
        if let Some(mut cmd)=state.ui.media_cmd.take() {
            if let media::MediaCmd::Play(_, rect)=&mut cmd { *rect=last.as_ref().and_then(|f|f.youtube_rect); }
            let _=media.send(cmd);
        }
        if let Some(cmd)=state.ui.spotify_cmd.take() {let _=spotify.send(cmd);}
        if let Some(n)=state.ui.git_more.take() {let _=local.send(sources::Local::More(n));}
        if state.ui.screenshot {state.ui.screenshot=false; let _=capture.send(());}
        if state.ui.open_picker {state.ui.open_picker=false; let _=picker.send(std::time::Instant::now());}
        if state.ui.open_profiles {state.ui.open_profiles=false; let _=profiles.send(std::time::Instant::now());}
        if let Some(name)=state.ui.use_profile.take() {
            // The click goes through the session profile channel: the name is data on a
            // 0600 socket, never text typed into the agent pane. The worker throttles
            // repeats and the reply arrives as an Event::ProfileChannel.
            sources::profile_click(name);
        }
        if state.ui.save {state.ui.save=false; if let Some(p)=state.ui.config_path.clone() {let err=config::save(&p,&state.ui.config).err(); if err!=state.ui.save_error {state.ui.save_error=err; draw=true;}}}
        if state.read_session {state.read_session=false; let _=local.send(sources::Local::Session(state.session_path.clone()));}
        if state.refresh {state.refresh=false; scanned=None; let _=local.send(sources::Local::Refresh); let _=pr.send(()); ask_profile();}
        // Leaving the tab forgets the scan, so coming back shows the tree as it is now.
        if state.ui.tab!=ui::Tab::Tree {scanned=None;}
        else if scanned.as_ref()!=Some(&state.ui.tree_open) {let _=trees.send(state.ui.tree_open.clone()); scanned=Some(state.ui.tree_open.clone());}
        if state.quit {break}
        if draw {
        let current = if state.ui.tab == ui::Tab::Media && state.media.spotify.cover_png.is_some() { state.media.spotify.cover_url.as_deref() } else { None };
        if transmitted.as_deref() != current { if transmitted.take().is_some() { write!(io::stdout(), "{COVER_DELETE}")?; } }
        else if transmitted.is_some() { write!(io::stdout(), "{COVER_UNPLACE}")?; }
        terminal.draw(|f|{
            let area=f.area(); let frame=ui::frame(&state,area.width,area.height);
            let lines:Vec<Line>=frame.lines.iter().map(|l|Line::from(l.iter().map(|g|{let (r,gr,b)=g.tone.rgb(); let st=Style::default().fg(Color::Rgb(r,gr,b)); Span::styled(g.text.clone(),if g.bold {st.add_modifier(Modifier::BOLD)} else {st})}).collect::<Vec<_>>())).collect();
            f.render_widget(Paragraph::new(lines),area); last=Some(frame);
        })?;
        // The video window may only exist while the rail is actually showing its block: another tab of
        // the rail, a collapsed card or one scrolled out of view all mean "not on screen".
        let card = last.as_ref().and_then(|f| f.youtube_rect);
        if sent_card != Some(card) { sent_card = Some(card); state.ui.media_cmd = Some(media::MediaCmd::CardRect(card)); }
        if let (Some((x,y)), Some(url), Some(bytes)) = (last.as_ref().and_then(|f|f.cover_pos), current, state.media.spotify.cover_png.as_deref()) {
            let mut out = io::stdout().lock();
            if transmitted.is_none() {
                let encoded = cover::base64(bytes);
                for (i, (chunk, more)) in cover::chunks(&encoded).into_iter().enumerate() {
                    if i == 0 { write!(out, "\x1b_Ga=t,f=100,i=731951,q=2,m={};{}\x1b\\", u8::from(more), chunk)?; }
                    else { write!(out, "\x1b_Gm={};{}\x1b\\", u8::from(more), chunk)?; }
                }
                transmitted = Some(url.to_owned());
            }
            write!(out, "\x1b7\x1b[{};{}H\x1b_Ga=p,i=731951,p=1,c=12,r=6,C=1,q=2\x1b\\\x1b8", y+1, x+1)?;
            out.flush()?;
        }
        }
        draw=false;
        // At the end of the GIT body the next page is asked for once: the wheel is the only "more" needed,
        // and the answer's own limit is what stops the asking.
        if state.ui.tab==ui::Tab::Git && state.ui.git_view==ui::GitView::Graph {
            let exhausted=state.git.commits.len()<state.git.log_limit;
            // The window is at the end of what was fetched, not the page: the wheel went as far back as the
            // log it asked for goes.
            let bottom=state.ui.git_scroll+ui::GRAPH_ROWS>=state.git.graph.len();
            if bottom && !exhausted && state.git.log_limit<sources::LOG_MAX && asked_page!=Some(state.git.log_limit) {
                asked_page=Some(state.git.log_limit);
                state.ui.git_more=Some((state.git.log_limit*2).min(sources::LOG_MAX));
            }
        }
        match rx.recv()? {
            Message::Input(Input::Resize(..))=>draw=true,
            Message::Input(Input::Key(k)) if k.kind!=KeyEventKind::Release => {
                let key=match k.code {
                    KeyCode::Char('c') if k.modifiers.contains(KeyModifiers::CONTROL)=>break,
                    KeyCode::Char(c) if !k.modifiers.intersects(KeyModifiers::CONTROL|KeyModifiers::ALT)=>Some(ui::Key::Char(c)),
                    KeyCode::Backspace=>Some(ui::Key::Back), KeyCode::Enter=>Some(ui::Key::Enter), KeyCode::Esc=>Some(ui::Key::Esc),
                    _=>None
                };
                if let Some(key)=key {draw=apply(&mut state,Event::Key(key));}
            }
            Message::Input(Input::Mouse(m))=>{
                let (col,row)=(usize::from(m.column),usize::from(m.row));
                let action=last.as_ref().and_then(|f|match m.kind {
                    MouseEventKind::Down(MouseButton::Left)=>ui::click(f,col,row),
                    MouseEventKind::ScrollDown=>ui::wheel(f,row,true), MouseEventKind::ScrollUp=>ui::wheel(f,row,false),
                    _=>None
                });
                if let Some(a)=action {draw=apply(&mut state,Event::Ui(a));}
            }
            Message::Data(e)=>{
                // A branch or commit change means HEAD moved; dirty-only updates don't fetch PRs.
                let git=matches!(e,Event::Git(_));
                if let Event::Git(g)=&e {if head_moved(&state.git,g) {let _=pr.send(());}}
                draw=apply(&mut state,e);
                if git && draw {asked=None; scanned=None;}
                if git {asked_page=None;}
            }
            _=>{}
        }
        let want=request(&state); if want!=asked {if let Some(r)=&want {let _=details.send(r.clone());} asked=want;}
        // TREE: read the picked file only while its tab is on screen. The answer is kept only for the file
        // still picked, so a click never shows the file clicked before it.
        let pick=(state.ui.tab==ui::Tab::Tree).then(||state.ui.tree_pick.clone()).flatten();
        if pick!=asked_preview {if let Some(rel)=&pick {let _=previews.send(rel.clone());} asked_preview=pick;}
        // One window for the whole rail, over the chat: 50 columns fit neither a diff nor an image.
        let show=ui::panel_show(&state);
        let key=show.as_ref().map(|(k,n,_)|(k.clone(),*n));
        if key!=shown {
            shown=key;
            match show {Some((_,_,cmd))=>{let _=panel.send(cmd);}, None=>{let _=panel.send(sources::PanelCmd::Hide);}}
        }
    }
    Ok(())
}
