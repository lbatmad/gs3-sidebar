//! CUTE rail (pi-cinlodev-cute sidebar): banner, tab bar, double-framed collapsible cards,
//! scroll and mouse hit-test. Pure: `frame()` draws, `act()`/`key()` change only `Ui`.
use crate::config::{clean_name, Config, NAME_MAX};
use crate::core::*;
use crate::media::{MediaCmd, SpotifyCmd, REDIRECT};
use crate::sources::{PanelCmd, PREVIEW_MAX};
use crate::syntax;
use std::path::PathBuf;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab { #[default] Main, Git, Prof, Mem, Tree, Media, Config }
impl Tab {
    pub const ALL: [Tab; 7] = [Tab::Main, Tab::Git, Tab::Prof, Tab::Mem, Tab::Tree, Tab::Media, Tab::Config];
    pub fn index(self) -> usize { self as usize }
    /// Keys follow visual order: MEDIA takes 6 and the CONFIG gear moves to 7 (S10).
    pub fn key(self) -> char { (b'1' + self as u8) as char }
    pub fn label(self) -> &'static str { ["MAIN", "GIT", "PROF", "MEM", "TREE", "MEDIA", "CONFIG"][self.index()] }
    /// Tab-bar name (no numbers; CONFIG is its icon) and the two-letter fallback for narrow panes.
    /// The fallbacks stay distinct: MEM is `ME`, MEDIA is `MD`.
    pub fn name(self, short: bool) -> &'static str {
        if short { ["MA", "GI", "PR", "ME", "TR", "MD", CONFIG_ICON][self.index()] } else { ["MAIN", "GIT", "PROF", "MEM", "TREE", "MEDIA", CONFIG_ICON][self.index()] }
    }
    pub fn id(self) -> &'static str { ["main", "git", "prof", "mem", "tree", "media", "config"][self.index()] }
    pub fn from_id(id: &str) -> Option<Tab> { Self::ALL.into_iter().find(|t| t.id() == id) }
    pub fn from_key(c: char) -> Option<Tab> { Self::ALL.into_iter().find(|t| t.key() == c) }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ui {
    pub tab: Tab, pub config: Config, pub scroll: [usize; 7],
    /// Name being typed in CONFIG (`None` when not editing).
    pub editing: Option<String>,
    /// Where CONFIG is persisted (`--config`); `None` keeps changes in memory only.
    pub config_path: Option<PathBuf>, pub save: bool, pub save_error: Option<String>,
    /// GIT tab: graph or branches view, branch being diffed against HEAD, file whose diff is previewed.
    pub git_view: GitView, pub git_branch: Option<String>, pub git_file: Option<String>, pub git_commit: Option<String>,
    /// A bigger page of the graph was asked for by the wheel or by the card's own row.
    pub git_more: Option<usize>,
    pub commit_message: String, pub editing_commit_message: Option<String>,
    /// GIT · Cambios: the write a click asked for (path + whether to stage), a bulk staging, or a
    /// commit. main consumes them off the render path and the worker throttles repeats.
    pub git_stage: Option<(String, bool)>, pub git_stage_all: Option<bool>, pub git_commit_now: bool,
    /// TREE tab: expanded folders (sorted, root-relative) and the selected file.
    pub tree_open: Vec<String>, pub tree_pick: Option<String>,
    /// Explicit model-section click; main consumes this intent off the render path.
    pub open_picker: bool,
    pub voice_cmd: Option<VoiceCmd>,
    /// MEDIA: the YouTube search box while it is open, and the command the engine consumes off the render path.
    pub prompt: Option<String>, pub spotify_prompt: Option<String>, pub media_cmd: Option<MediaCmd>, pub spotify_cmd: Option<SpotifyCmd>,
    /// MEDIA · `⌁ Pistas`: index of the first track row the card shows. A playlist longer than the
    /// card's window scrolls here, inside its own frame, instead of pushing YouTube out of reach.
    pub track_scroll: usize,
    /// Which 25 rows of the graph the GIT card is showing.
    pub git_scroll: usize,
    pub yt_lists: Option<String>, pub video_scroll: usize,
    /// Explicit click on the Flameshot icon; main consumes this intent off the render path.
    pub screenshot: bool,
    /// PROF: the profile name a click left selected. Display only: the HUD never applies it.
    pub profile_pick: Option<String>,
    /// PROF: an explicit click asked for the `/gentle:profiles` screen; main consumes this intent
    /// off the render path and the pane worker throttles repeats.
    pub open_profiles: bool,
    /// PROF: the profile a click asked to bind to the current session (`/profile <name>`, a
    /// session-only binding); main hands it to a per-name pane worker.
    pub use_profile: Option<String>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GitView { #[default] Graph, Branches }
impl Ui {
    pub fn new(config: Config, config_path: Option<PathBuf>) -> Self {
        let tab = Tab::ALL.into_iter().find(|t| config.visible(*t)).unwrap_or(Tab::Config);
        Ui { tab, config, config_path, ..Ui::default() }
    }
    pub fn tabs(&self) -> Vec<Tab> { Tab::ALL.into_iter().filter(|t| self.config.visible(*t)).collect() }
    pub fn collapsed(&self, id: &str) -> bool { self.config.collapsed.iter().any(|c| c == id) }
}

/// What a click, wheel step or key does. Pure intents: nothing here performs I/O.
#[derive(Clone, Debug, PartialEq)]
pub enum Action { Tab(Tab), Cycle(i32), Collapse(String), Hide(Tab), Edit, EditCommitMessage, ScrollTo(usize), GitView(GitView), Branch(Option<String>), File(String),
    /// MEDIA · `⌁ Pistas`: the index of the first track row to show, so the wheel can scroll a long
    /// playlist inside its card.
    ScrollList(usize), ScrollVideos(usize), ScrollGraph(usize), YtLists, YtOpenPlaylist(String), YtPlayVideo(usize), Commit(String),
    /// GIT · Cambios: stage/unstage one path, stage/unstage everything, or commit the index.
    Stage(String), Unstage(String), StageAll, UnstageAll, CommitNow,
    /// TREE: toggle a folder, fold every folder, open a changed file's diff in GIT, select an unchanged file.
    Dir(String), Fold, Inspect(String), Pick(String), MoreCommits(usize), ModelPicker, VoiceMute, VoiceDeaf, VoiceOpen, Screenshot,
    /// PROF: a click on a profile row; the HUD only keeps the name visible for the human.
    PickProfile(String),
    /// PROF: open the `/gentle:profiles` screen on the bound agent pane (text + Enter via herdr).
    ProfilesCommand,
    /// PROF: bind this profile to the current session through the agent pane (`/profile <name>`).
    UseProfile(String),
    /// MEDIA · YouTube: open the search box, transport, and a click on the seek bar as a 0.0–1.0 fraction.
    Search, PlayPause, PrevTrack, NextTrack, Seek(f64),
    /// MEDIA · Spotify: consent in the browser, renew the stored token, list playlists, open one.
    SpotifyAuthorize, SpotifyRefresh, SpotifyPlaylists, SelectPlaylist(String), SpotifySearch, SpotifyPlayResult(usize), SpotifyStart, SpotifyPlay(usize), SpotifyPause, SpotifyResume, SpotifyNext, SpotifyPrevious, SpotifySeek(f64), SpotifyVolume(u64), SpotifyShuffle(bool), SpotifyRepeat(crate::media::Repeat) }
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Key { Char(char), Back, Enter, Esc }

pub fn act(ui: &mut Ui, a: Action) {
    if matches!(a, Action::Search) { ui.spotify_prompt = None; ui.yt_lists = None; }
    match a {
        Action::Tab(t) if ui.config.visible(t) => { if ui.tab != t { ui.editing = None; ui.editing_commit_message = None; } ui.tab = t; }
        Action::Cycle(d) => {
            let tabs = ui.tabs(); let i = tabs.iter().position(|t| *t == ui.tab).unwrap_or(0) as i32;
            act(ui, Action::Tab(tabs[(i + d).rem_euclid(tabs.len() as i32) as usize]));
        }
        Action::Collapse(id) => {
            let c = &mut ui.config.collapsed;
            match c.iter().position(|x| *x == id) { Some(i) => { c.remove(i); } None => { c.push(id); c.sort(); } }
            ui.save = true;
        }
        Action::Hide(t) if t != Tab::Config => {
            let h = &mut ui.config.hidden;
            match h.iter().position(|x| *x == t) { Some(i) => { h.remove(i); } None => { h.push(t); h.sort_by_key(|t| t.index()); } }
            ui.save = true;
        }
        Action::ModelPicker => ui.open_picker = true,
        Action::VoiceMute => ui.voice_cmd = Some(VoiceCmd::Mute),
        Action::VoiceDeaf => ui.voice_cmd = Some(VoiceCmd::Deaf),
        Action::VoiceOpen => ui.voice_cmd = Some(VoiceCmd::Open),
        Action::Screenshot => ui.screenshot = true,
        Action::Search => ui.prompt = Some(String::new()),
        Action::YtLists => { ui.prompt = None; ui.spotify_prompt = None; ui.yt_lists = Some(String::new()); }
        Action::YtOpenPlaylist(id) => { ui.video_scroll = 0; ui.media_cmd = Some(MediaCmd::OpenPlaylist(id)); }
        Action::YtPlayVideo(row) => ui.media_cmd = Some(MediaCmd::PlayVideoAt(row)),
        Action::ScrollVideos(n) => ui.video_scroll = n,
        Action::PlayPause => ui.media_cmd = Some(MediaCmd::Toggle),
        Action::NextTrack => ui.media_cmd = Some(MediaCmd::Next),
        Action::PrevTrack => ui.media_cmd = Some(MediaCmd::Prev),
        Action::Seek(fraction) => ui.media_cmd = Some(MediaCmd::Seek(fraction)),
        Action::SpotifyAuthorize => ui.spotify_cmd = Some(SpotifyCmd::Authorize),
        Action::SpotifyRefresh => ui.spotify_cmd = Some(SpotifyCmd::Refresh),
        Action::SpotifyPlaylists => ui.spotify_cmd = Some(SpotifyCmd::Playlists),
        Action::SelectPlaylist(id) => { ui.track_scroll = 0; ui.spotify_cmd = Some(SpotifyCmd::Select(id)); }
        Action::SpotifySearch => { ui.prompt = None; ui.yt_lists = None; ui.spotify_prompt = Some(String::new()); }
        Action::SpotifyPlayResult(row) => ui.spotify_cmd = Some(SpotifyCmd::PlayResult(row)),
        Action::SpotifyStart => ui.spotify_cmd = Some(SpotifyCmd::Start),
        Action::SpotifyPlay(row) => ui.spotify_cmd = Some(SpotifyCmd::Play(row)),
        Action::SpotifyPause => ui.spotify_cmd = Some(SpotifyCmd::Pause),
        Action::SpotifyResume => ui.spotify_cmd = Some(SpotifyCmd::Resume),
        Action::SpotifyNext => ui.spotify_cmd = Some(SpotifyCmd::Next),
        Action::SpotifyPrevious => ui.spotify_cmd = Some(SpotifyCmd::Previous),
        Action::SpotifySeek(fraction) => ui.spotify_cmd = Some(SpotifyCmd::Seek(fraction)),
        Action::SpotifyVolume(percent) => ui.spotify_cmd = Some(SpotifyCmd::Volume(percent)),
        Action::SpotifyShuffle(on) => ui.spotify_cmd = Some(SpotifyCmd::Shuffle(on)),
        Action::SpotifyRepeat(repeat) => ui.spotify_cmd = Some(SpotifyCmd::Repeat(repeat)),
        Action::Edit => ui.editing = Some(ui.config.name.clone()),
        Action::EditCommitMessage => ui.editing_commit_message = Some(ui.commit_message.clone()),
        // The gates live here as well as in the card: a hand-built event must not stage a
        // sensitive path or commit an empty index.
        Action::Stage(path) if !crate::core::sensitive(&path) => ui.git_stage = Some((path, true)),
        Action::Unstage(path) => ui.git_stage = Some((path, false)),
        Action::StageAll => ui.git_stage_all = Some(true),
        Action::UnstageAll => ui.git_stage_all = Some(false),
        Action::CommitNow if ui.commit_message.trim().len() > 0 => ui.git_commit_now = true,
        Action::ScrollTo(n) => ui.scroll[ui.tab.index()] = n,
        Action::ScrollList(n) => ui.track_scroll = n,
        Action::ScrollGraph(n) => ui.git_scroll = n,
        Action::GitView(v) => ui.git_view = v,
        // Clicking the selected branch again (or ✕) closes the inspector and its file.
        Action::Branch(b) => { ui.git_file = None; ui.git_commit = None; ui.git_branch = if b.is_some() && b == ui.git_branch { None } else { b }; }
        // A commit opens its own file list; clicking the same commit again closes it.
        Action::Commit(h) => {
            if !crate::core::is_hash(&h) { return }
            ui.git_file = None; ui.git_branch = None;
            ui.git_commit = if ui.git_commit.as_deref() == Some(h.as_str()) { None } else { Some(h) };
        }
        Action::File(f) => ui.git_file = if ui.git_file.as_ref() == Some(&f) { None } else { Some(f) },
        Action::Dir(d) => { let o = &mut ui.tree_open; match o.iter().position(|x| *x == d) { Some(i) => { o.remove(i); } None => { o.push(d); o.sort(); } } }
        Action::Fold => ui.tree_open.clear(),
        Action::MoreCommits(n) => ui.git_more = Some(n.min(crate::sources::LOG_MAX)),
        // Navigation only: the diff opens inside the HUD, in the GIT tab.
        Action::Inspect(p) => { ui.tree_pick = Some(p.clone()); ui.git_branch = None; ui.git_file = Some(p); act(ui, Action::Tab(Tab::Git)); }
        Action::Pick(p) => ui.tree_pick = if ui.tree_pick.as_ref() == Some(&p) { None } else { Some(p) },
        Action::PickProfile(p) => ui.profile_pick = if ui.profile_pick.as_ref() == Some(&p) { None } else { Some(p) },
        Action::ProfilesCommand => ui.open_profiles = true,
        // The name in the intent already passed the sendable check at render time; act() re-checks
        // so a hand-built event can never smuggle an argument into the pane.
        Action::UseProfile(name) if crate::core::is_valid_profile_name(&name) => ui.use_profile = Some(name),
        _ => {}
    }
}
pub fn key(s: &mut State, k: Key) {
    if let Some(buf) = &mut s.ui.editing_commit_message {
        match k {
            Key::Char(c) if !c.is_control() && buf.chars().count()<1024 => buf.push(c),
            Key::Back => { buf.pop(); }
            Key::Enter => { s.ui.commit_message=buf.clone(); s.ui.editing_commit_message=None; }
            Key::Esc => s.ui.editing_commit_message=None,
            _ => {}
        }
        return;
    }
    if let Some(buf) = &mut s.ui.editing {
        match k {
            Key::Char(c) if !c.is_control() && buf.chars().count() < NAME_MAX => buf.push(c),
            Key::Back => { buf.pop(); }
            Key::Enter => { s.ui.config.name = clean_name(buf); s.ui.editing = None; s.ui.save = true; }
            Key::Esc => s.ui.editing = None,
            _ => {}
        }
        return;
    }
    if let Some(buf) = &mut s.ui.yt_lists {
        match k {
            Key::Char(c) if !c.is_control() && buf.chars().count() < 120 => buf.push(c),
            Key::Back => { buf.pop(); }
            Key::Enter => { let q = buf.trim().to_string(); s.ui.yt_lists = None; if !q.is_empty() { s.ui.media_cmd = Some(MediaCmd::SearchPlaylists(q)); } }
            Key::Esc => s.ui.yt_lists = None,
            _ => {}
        }
        return;
    }
    // Spotify search is separate so the YouTube modal keeps its existing behavior.
    if let Some(buf) = &mut s.ui.spotify_prompt {
        match k {
            Key::Char(c) if !c.is_control() && buf.chars().count() < 120 => buf.push(c),
            Key::Back => { buf.pop(); }
            Key::Enter => { let query = buf.trim().to_string(); s.ui.spotify_prompt = None; if !query.is_empty() { s.ui.spotify_cmd = Some(SpotifyCmd::Search(query)); } }
            Key::Esc => s.ui.spotify_prompt = None,
            _ => {}
        }
        return;
    }
    // MEDIA · YouTube search box: modal while open, so `q` and the tab keys are text here.
    if let Some(buf) = &mut s.ui.prompt {
        match k {
            Key::Char(c) if !c.is_control() && buf.chars().count() < 120 => buf.push(c),
            Key::Back => { buf.pop(); }
            Key::Enter => {
                let query = buf.trim().to_string(); s.ui.prompt = None;
                if !query.is_empty() { s.ui.media_cmd = Some(MediaCmd::Play(query, None)); }
            }
            Key::Esc => s.ui.prompt = None,
            _ => {}
        }
        return;
    }
    match k {
        Key::Char('q') => s.quit = true,
        Key::Char('r') => s.refresh = true,
        Key::Char(c) => if let Some(t) = Tab::from_key(c) { act(&mut s.ui, Action::Tab(t)) },
        _ => {}
    }
}

// ---- width-safe segments --------------------------------------------------------------
#[derive(Clone, Debug, PartialEq)]
pub struct Seg { pub text: String, pub tone: Tone, pub bold: bool }
pub fn seg(text: impl Into<String>, tone: Tone) -> Seg { Seg { text: text.into(), tone, bold: false } }
fn bold(text: impl Into<String>, tone: Tone) -> Seg { Seg { text: text.into(), tone, bold: true } }
/// CONFIG tab icon: the gearbox `⛭` (U+26ED, 1 column, monochrome), always purple.
pub const CONFIG_ICON: &str = "⛭";
/// Theme mark of the banner and card titles: the power symbol `⌁` (U+2301, 1 column, monochrome), always purple.
pub const MARK: &str = "⌁";
/// Card title `⌁ name`: purple mark, name in the card's tone.
fn mark(name: &str, tone: Tone) -> Vec<Seg> { vec![seg(MARK, Tone::Purple), seg(format!(" {name}"), tone)] }
/// A tab name; CONFIG is its purple icon.
fn tab_name(t: Tab, short: bool, tone: Tone, strong: bool) -> Seg { if t == Tab::Config { seg(CONFIG_ICON, Tone::Purple) } else { Seg { text: t.name(short).into(), tone, bold: strong } } }
pub fn plain(line: &[Seg]) -> String { line.iter().map(|g| g.text.as_str()).collect() }
fn width_of(line: &[Seg]) -> usize { line.iter().map(|g| g.text.width()).sum() }
/// A JSON field as display text: `""` when missing, never a panic.
fn text_of(v: &serde_json::Value, key: &str) -> String { v[key].as_str().unwrap_or("").to_string() }
/// The channel event carries the profile itself (`{name, models}`); a wrapped
/// `{ok, profile}` reply from an older worker must not blank the cards.
fn session_profile(v: &serde_json::Value) -> &serde_json::Value {
    v.get("profile").filter(|p| p.is_object()).unwrap_or(v)
}
fn fit(line: Vec<Seg>, width: usize) -> Vec<Seg> {
    let mut out = Vec::new(); let mut room = width;
    for g in line {
        if room == 0 { break }
        let t = truncate(&g.text, room); let whole = t.len() == g.text.len(); room -= t.width();
        out.push(Seg { text: t, ..g }); if !whole { break }
    }
    out
}
fn centered(line: Vec<Seg>, w: usize) -> Vec<Seg> {
    let line = fit(line, w); let pad = w.saturating_sub(width_of(&line)) / 2;
    let mut l = vec![seg(" ".repeat(pad), Tone::Text)]; l.extend(line); l
}

// ---- CUTE double frame ----------------------------------------------------------------
/// Title-bar controls drawn before `[-]` (e.g. `[ Grafo | Ramas ]`); each segment may be clickable.
type Extra = Vec<(Seg, Option<Action>)>;
/// `╔═ title ═…═ [extra] [-] ═╗`: the toggle is drawn when the card is wide enough for it,
/// the extra controls only while expanded and when the title keeps at least 4 columns.
fn top(title: Vec<Seg>, extra: &Extra, w: usize, collapsed: bool) -> (Vec<Seg>, Option<(usize, usize)>, Vec<(usize, usize, Action)>) {
    let toggle = w >= 16;
    let ew: usize = extra.iter().map(|(g, _)| g.text.width()).sum();
    let show = toggle && !collapsed && ew > 0 && w >= 16 + ew;
    let cost = if show { ew + 1 } else { 0 };
    let room = w.saturating_sub(if toggle { 11 + cost + usize::from(!show) } else { 6 });
    let title = fit(title, room); let tw = width_of(&title);
    let mut l = vec![seg("╔═ ", Tone::Border)]; l.extend(title);
    if toggle {
        let fill = w.saturating_sub(11 + cost + tw);
        l.push(seg(format!(" {} ", "═".repeat(fill)), Tone::Border));
        let mut hits = Vec::new();
        if show {
            let mut x = 3 + tw + 2 + fill;
            for (g, a) in extra { let n = g.text.width(); if let Some(a) = a { hits.push((x, x + n, a.clone())); } x += n; l.push(g.clone()); }
            l.push(seg(" ", Tone::Border));
        }
        l.push(seg("[", Tone::Dim)); l.push(bold(if collapsed { "+" } else { "-" }, Tone::PinkBright)); l.push(seg("]", Tone::Dim));
        l.push(seg(" ═╗", Tone::Border));
        (l, Some((w - 6, w - 3)), hits)
    } else {
        l.push(seg(format!(" {}╗", "═".repeat(w.saturating_sub(5 + tw))), Tone::Border)); (l, None, Vec::new())
    }
}
fn bottom(w: usize) -> Vec<Seg> { vec![seg(format!("╚{}╝", "═".repeat(w.saturating_sub(2))), Tone::Border)] }
fn divider(w: usize) -> Vec<Seg> { vec![seg(format!("╠{}╣", "═".repeat(w.saturating_sub(2))), Tone::Border)] }
fn pad(left: Vec<Seg>, right: Vec<Seg>, w: usize) -> Vec<Seg> {
    let inner = w.saturating_sub(4); let (lw, rw) = (width_of(&left), width_of(&right));
    let mut l = vec![seg("║ ", Tone::Border)]; l.extend(left); l.push(seg(" ".repeat(inner.saturating_sub(lw + rw)), Tone::Text)); l.extend(right); l.push(seg(" ║", Tone::Border)); l
}
/// `║ left ··· right ║`; the right side wins when space runs out.
fn row(left: Vec<Seg>, right: Vec<Seg>, w: usize) -> Vec<Seg> {
    let inner = w.saturating_sub(4);
    let right = fit(right, inner); let rw = width_of(&right);
    let left = fit(left, inner.saturating_sub(if rw > 0 { rw + 1 } else { 0 }));
    pad(left, right, w)
}
/// `║ Label ··· value ║`; the label stays whole and the value is cut.
fn keyed(label: &str, value: Vec<Seg>, w: usize) -> Vec<Seg> {
    let inner = w.saturating_sub(4);
    let left = fit(vec![seg(label, Tone::Gold)], inner); let lw = width_of(&left);
    let right = fit(value, inner.saturating_sub(lw + 1));
    pad(left, right, w)
}

/// One framed card: `id` keys its collapsed state, rows may carry click actions.
struct Card { id: &'static str, title: Vec<Seg>, extra: Extra, w: usize, rows: Vec<Vec<Seg>>, hits: Vec<(usize, usize, usize, Action)>, list: Option<(usize, usize)>, video: Option<std::ops::Range<usize>> }
impl Card {
    fn new(id: &'static str, title: Vec<Seg>, w: usize) -> Self { Card { id, title, extra: Vec::new(), w, rows: Vec::new(), hits: Vec::new(), list: None, video: None } }
    /// This card's rows are a window of a longer list: the wheel over it scrolls the list, not the body.
    fn scrolls(&mut self, scroll: usize, max_scroll: usize) { if max_scroll > 0 { self.list = Some((scroll, max_scroll)); } }
    fn row(&mut self, l: Vec<Seg>, r: Vec<Seg>) { self.rows.push(row(l, r, self.w)); }
    fn keyed(&mut self, label: &str, v: Vec<Seg>) { self.rows.push(keyed(label, v, self.w)); }
    /// Reserve the actual rows below the search controls for the external video window.
    fn video_rows(&mut self) {
        let start = self.rows.len();
        for _ in 0..VIDEO_ROWS { self.row(vec![], vec![]); }
        self.video = Some(start..self.rows.len());
    }
    fn divider(&mut self) { self.rows.push(divider(self.w)); }
    /// The whole inner width of the last row triggers `a`.
    fn on_click(&mut self, a: Action) { let i = self.rows.len() - 1; self.hits.push((i, 2, self.w.saturating_sub(2), a)); }
}

pub struct Hit { pub row: usize, pub x0: usize, pub x1: usize, pub action: Action }
/// What a card's own list is: the wheel scrolls the list the hand is over, and each one keeps its offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind { Tracks, Videos, Graph }
/// A card that scrolls its own rows: its first screen row, how many rows it shows, and where the list
/// stands. Screen rows are absolute, like [`Hit`], so the wheel can tell which card the pointer is on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ListWindow { pub top: usize, pub height: usize, pub scroll: usize, pub max_scroll: usize, pub kind: ListKind }
/// The rows of the graph the GIT card shows at once: it is a window, not the whole history.
pub const GRAPH_ROWS: usize = 25;
pub struct Frame { pub lines: Vec<Vec<Seg>>, pub hits: Vec<Hit>, pub tab_row: usize, pub scroll: usize, pub max_scroll: usize, pub cover_pos: Option<(usize, usize)>, pub youtube_rect: Option<crate::media::CellRect>, pub lists: Vec<ListWindow> }
/// The narrowest region under the cursor wins (a title toggle beats its whole row).
pub fn click(f: &Frame, col: usize, row: usize) -> Option<Action> {
    let best = f.hits.iter().filter(|h| h.row == row && (h.x0..h.x1).contains(&col)).min_by_key(|h| h.x1 - h.x0);
    // `GS3_HUD_DEBUG=1` records what answered a click and what else was on that row: a card whose
    // whole-row fallback is wider than the control under the cursor made "the wrong action won" a
    // guess until the candidates were written down.
    if std::env::var_os("GS3_HUD_DEBUG").is_some() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!("gs3-hud-{}-debug.log", std::process::id()));
        if let Ok(mut log) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let row_hits: Vec<String> = f_row_hits(f, row);
            let _ = writeln!(log, "click col={col} row={row} winner={:?} row_hits=[{}]", best.map(|h| &h.action), row_hits.join(" | "));
        }
    }
    best.map(|h| h.action.clone())
}

/// `(x0, x1, action)` for every hit on one screen row, widest first, for the click trace.
fn f_row_hits(f: &Frame, row: usize) -> Vec<String> {
    let mut v: Vec<(usize, String)> = f.hits.iter().filter(|h| h.row == row)
        .map(|h| (h.x1 - h.x0, format!("{}..{} {:?}", h.x0, h.x1, h.action))).collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|(_, s)| s).collect()
}
/// Wheel over the tab bar cycles tabs; a card that owns a longer list takes it over its own rows;
/// anywhere else scrolls the body (clamped, 3 lines a step).
pub fn wheel(f: &Frame, row: usize, down: bool) -> Option<Action> {
    if row <= f.tab_row + 1 && row + 1 >= f.tab_row { return Some(Action::Cycle(if down { 1 } else { -1 })) }
    // A list is what the human is aiming at, and letting the body move instead would carry the card
    // away before they ever see row eleven. Past its own ends it hands the wheel back, or the
    // sections below it would be unreachable.
    if let Some(l) = f.lists.iter().find(|l| (l.top..l.top + l.height).contains(&row)) {
        let n = if down { (l.scroll + 3).min(l.max_scroll) } else { l.scroll.saturating_sub(3) };
        if n != l.scroll { return Some(match l.kind { ListKind::Graph => Action::ScrollGraph(n), ListKind::Videos => Action::ScrollVideos(n), ListKind::Tracks => Action::ScrollList(n) }) }
    }
    let n = if down { (f.scroll + 3).min(f.max_scroll) } else { f.scroll.saturating_sub(3) };
    (n != f.scroll).then_some(Action::ScrollTo(n))
}

const PAD: usize = 1;
/// The rows the YouTube card reserves for the video window. The picture takes that block whole — the
/// card's inner width by these twelve rows, which is the closest a 42-46 column card gets to 16:9
/// (368x204 px on this rail, 1.8:1), so mpv has almost nothing to crop and the window lands inside the
/// frame instead of over it. Margins around the picture, horizontal or vertical, only made sense while
/// the window was drawn 15% too wide: with true cells they are empty bands beside a square video.
const VIDEO_ROWS: usize = 12;
const VIDEO_ROWS_TALL: usize = VIDEO_ROWS;
/// One cell of air on each side of the picture, the margin the human asked for by name: full bleed
/// looked like a wall and sixteen percent looked like a square in the middle of the card.
const VIDEO_SIDE: usize = 1;
struct Body { lines: Vec<Vec<Seg>>, hits: Vec<Hit>, glued: bool, lists: Vec<ListWindow>, video: Option<std::ops::Range<usize>> }
impl Body {
    fn push(&mut self, ui: &Ui, c: Card) {
        if !self.lines.is_empty() && !std::mem::take(&mut self.glued) { self.lines.push(Vec::new()); }
        let collapsed = ui.collapsed(c.id); let start = self.lines.len();
        let (line, toggle, extra) = top(c.title, &c.extra, c.w, collapsed); self.lines.push(line);
        for (x0, x1, a) in extra { self.hits.push(Hit { row: start, x0: PAD + x0, x1: PAD + x1, action: a }); }
        let toggle_at = |(a, b): (usize, usize)| Hit { row: start, x0: PAD + a, x1: PAD + b, action: Action::Collapse(c.id.into()) };
        self.hits.push(Hit { row: start, x0: PAD, x1: PAD + c.w, action: Action::Collapse(c.id.into()) });
        if let Some(t) = toggle { self.hits.push(toggle_at(t)); }
        if !collapsed {
            if let Some(range) = &c.video { self.video = Some(start + 1 + range.start..start + 1 + range.end); }
            // Title included: the whole visible card takes the wheel, which is where the hand is.
            if let Some((scroll, max_scroll)) = c.list { let kind = match c.id { "videos" => ListKind::Videos, "git" => ListKind::Graph, _ => ListKind::Tracks };
            self.lists.push(ListWindow { top: start, height: c.rows.len() + 1, scroll, max_scroll, kind }); }
            for (i, x0, x1, a) in c.hits { self.hits.push(Hit { row: start + 1 + i, x0: PAD + x0, x1: PAD + x1, action: a }); }
            self.lines.extend(c.rows);
        }
        self.lines.push(bottom(c.w));
    }
}

fn tab_bar(ui: &Ui, s: &State, w: usize) -> (Vec<Seg>, Vec<(usize, usize, Tab)>) {
    let sync = match s.git.ahead_behind { Some((a, b)) if a + b > 0 => format!(" {}{}", if a > 0 { format!("▲{a}") } else { String::new() }, if b > 0 { format!("▼{b}") } else { String::new() }), _ => String::new() };
    let tabs = ui.tabs(); let compact = w < 44;
    // Names only (S8): `[ MAIN ]` active (`[MAIN]` under 44 columns), the others muted; CONFIG is the gear.
    let visual = |short: bool| -> Vec<(Tab, Vec<Seg>)> {
        tabs.iter().map(|&t| {
            let extra = if t == Tab::Git { sync.as_str() } else { "" };
            if t == ui.tab {
                let (open, close) = if compact { ("[", "]") } else { ("[ ", " ]") };
                (t, vec![bold(open, Tone::PinkBright), tab_name(t, short, Tone::PinkBright, true), bold(format!("{extra}{close}"), Tone::PinkBright)])
            } else { (t, vec![tab_name(t, short, Tone::Muted, false), seg(extra, Tone::Yellow)]) }
        }).collect()
    };
    // Too narrow for the names: two-letter names, so every tab stays visible and clickable.
    let mut items = visual(false);
    let needed = |items: &[(Tab, Vec<Seg>)]| items.iter().map(|(_, v)| width_of(v)).sum::<usize>() + items.len().saturating_sub(1);
    if needed(&items) > w { items = visual(true); }
    let total: usize = items.iter().map(|(_, v)| width_of(v)).sum();
    let gaps = items.len().saturating_sub(1); let rest = w.saturating_sub(total);
    let (base, mut extra) = if gaps > 0 { (rest / gaps, rest % gaps) } else { (0, 0) };
    let (mut line, mut boxes, mut x) = (Vec::new(), Vec::new(), 0);
    for (i, (t, v)) in items.into_iter().enumerate() {
        let len = width_of(&v); boxes.push((PAD + x, PAD + (x + len).min(w), t)); x += len; line.extend(v);
        if i < gaps { let g = base + usize::from(extra > 0); extra = extra.saturating_sub(1); line.push(seg(" ".repeat(g), Tone::Text)); x += g; }
    }
    (fit(line, w), boxes.into_iter().filter(|b| b.0 < PAD + w).collect())
}

/// Draw the rail for a `width`×`height` pane; `height` 0 draws everything (no scrolling).
pub fn frame(s: &State, width: u16, height: u16) -> Frame {
    use Tone::*;
    let w = width as usize; let net = w.saturating_sub(2 * PAD); let ui = &s.ui;
    let name = if ui.config.name.is_empty() { " - - ".to_string() } else { format!(" {} ", ui.config.name) };
    let mut head: Vec<Vec<Seg>> = vec![Vec::new(),
        centered(vec![seg(MARK, Purple), bold(name, PinkBright), seg(MARK, Purple)], net),
        centered(vec![seg("GENTLE-SHELL", Text)], net), Vec::new()];
    let (tabs, boxes) = tab_bar(ui, s, net); let tab_row = head.len(); head.push(tabs);
    let mut hits: Vec<Hit> = boxes.into_iter().map(|(x0, x1, t)| Hit { row: tab_row, x0, x1, action: Action::Tab(t) }).collect();
    head.push(vec![seg("─".repeat(net), BorderMuted)]);
    // MAIN: the voice icons sit right on the divider; other tabs keep one blank line.
    if ui.tab != Tab::Main { head.push(Vec::new()); }
    let top_rows = head.len();

    let mut body = Body { lines: Vec::new(), hits: Vec::new(), glued: false, lists: Vec::new(), video: None };
    let cw = net;

    // The cover owns six rows at the top of the Spotify card and the image is only placed while that
    // block is still fully on screen, so one decision feeds both. The scroll is the wheel's, not the
    // body's, which is what lets it be taken here: a card that reserved the rows after the image was
    // withheld (scrolled card, block out of the viewport) left an empty hole where the cover used to be.
    let view_hint = if height == 0 { usize::MAX } else { (height as usize).saturating_sub(top_rows) };
    let scroll_hint = ui.scroll[ui.tab.index()];
    let show_cover = ui.tab == Tab::Media && !ui.collapsed("spotify") && cw >= 16 && scroll_hint <= 1
        && 7 <= scroll_hint.saturating_add(view_hint)
        && crate::cover::ready(s.media.spotify.cover_url.as_deref(), s.media.spotify.cover_png.as_deref(), true, true);
    match ui.tab {
        Tab::Main => { voice_row(&mut body, s.voice, cw, ui.config.plain_controls); body.push(ui, status_card(s, cw)); body.push(ui, context_card(s, cw)); body.push(ui, todos_card(s, cw)); }
        Tab::Git => { body.push(ui, git_card(s, cw)); if let Some(c) = commit_card(s, cw) { body.push(ui, c); } body.push(ui, changes_card(s, cw)); body.push(ui, worktree_card(s, cw)); body.push(ui, pr_card(s, cw)); }
        Tab::Prof => { if let Some(c) = health_card(s, cw) { body.push(ui, c); } if let Some(c) = profile_store_card(s, cw) { body.push(ui, c); } body.push(ui, profiles_card(s, cw)); }
        Tab::Mem => { for c in [engram_card(s, cw), handoff_card(s, cw), stack_card(s, cw), tools_card(s, cw), mcp_card(s, cw)] { body.push(ui, c); } }
        Tab::Tree => body.push(ui, tree_card(s, cw)),
        Tab::Media => { body.push(ui, spotify_card(s, cw, show_cover)); if let Some(c) = tracks_card(s, cw, ui.track_scroll) { body.push(ui, c); } body.push(ui, youtube_card(s, cw)); if let Some(c) = videos_card(s, cw) { body.push(ui, c); } }
        Tab::Config => { for c in config_cards(s, cw) { body.push(ui, c); } }
    }
    if body.lines.is_empty() { body.lines.push(centered(vec![seg("Sin información disponible en esta pestaña.", Dim)], net)); }

    let view = if height == 0 { body.lines.len() } else { (height as usize).saturating_sub(head.len()) };
    let max_scroll = body.lines.len().saturating_sub(view);
    let scroll = ui.scroll[ui.tab.index()].min(max_scroll);
    if max_scroll > 0 {
        // More above/below: arrows at the right end of the divider.
        let mark = match (scroll > 0, scroll < max_scroll) { (true, true) => " ↑↓", (true, false) => " ↑", _ => " ↓" };
        head[tab_row + 1] = vec![seg("─".repeat(net.saturating_sub(mark.width())), BorderMuted), seg(mark, Muted)];
    }
    hits.extend(body.hits.into_iter().filter(|h| (scroll..scroll + view).contains(&h.row)).map(|h| Hit { row: h.row - scroll + top_rows, ..h }));
    // The same mapping for the cards that own a list, clipped to what the body actually shows: a list
    // half scrolled out only owns the rows still on screen.
    let lists = body.lists.into_iter().filter_map(|l| {
        let top = l.top.max(scroll); let end = (l.top + l.height).min(scroll + view);
        (end > top).then(|| ListWindow { top: top - scroll + top_rows, height: end - top, scroll: l.scroll, max_scroll: l.max_scroll, kind: l.kind })
    }).collect();
    let video = body.video;
    let lines = head.into_iter().chain(body.lines.into_iter().skip(scroll).take(view))
        .map(|l| { if l.is_empty() { return l } let mut p = vec![seg(" ".repeat(PAD), Text)]; p.extend(l); fit(p, w) }).collect();
    let cover_pos = show_cover.then(|| (PAD + 2, top_rows + 1 - scroll));
    let youtube_rect = video.filter(|r| cw > 4 && r.start >= scroll && r.end <= scroll + view).map(|r| {
        let inner = cw.saturating_sub(2);
        crate::media::CellRect::new((PAD + 1 + VIDEO_SIDE) as u32, (top_rows + r.start - scroll) as u32, inner.saturating_sub(VIDEO_SIDE * 2).max(1) as u32, VIDEO_ROWS_TALL.min(r.len()) as u32)
    });
    Frame { lines, hits, tab_row, scroll, max_scroll, cover_pos, youtube_rect, lists }
}
pub fn render_lines(s: &State, width: u16) -> Vec<String> { frame(s, width, 0).lines.iter().map(|l| plain(l)).collect() }

/// Material Design Nerd Font controls, regular weight in fixed five-cell slots.
/// Requires Nerd Font coverage unless plain_controls selects ASCII labels.
const MIC_ICON: &str = "\u{f036e}";
const MIC_OFF: &str = "\u{f036d}";
const HEADPHONES_ICON: &str = "\u{f02cb}";
const HEADPHONES_OFF: &str = "\u{f07ce}";
const DISCORD_ICON: &str = "\u{f066f}";
const CAPTURE_ICON: &str = "\u{f019f}";
fn voice_row(body: &mut Body, voice: Voice, w: usize, plain_controls: bool) {
    const ICON: usize = 5; const GAP: usize = 3;
    let tone = |active| if !voice.online { Tone::Dim } else if active { Tone::Coral } else { Tone::Mint };
    let icons = if plain_controls {
        [if voice.mute || voice.deaf { "MUTE" } else { "MIC" }, if voice.deaf { "DEAF" } else { "HP" }, "APP", "CAP"]
    } else {
        [if voice.mute || voice.deaf { MIC_OFF } else { MIC_ICON }, if voice.deaf { HEADPHONES_OFF } else { HEADPHONES_ICON }, DISCORD_ICON, CAPTURE_ICON]
    };
    let tones = [tone(voice.mute || voice.deaf), tone(voice.deaf), if voice.online { Tone::Secondary } else { Tone::Dim }, Tone::Secondary];
    let start = body.lines.len();
    let count = if w >= 4 * ICON + 3 * GAP { 4 } else { 3 };
    let row_w = count * ICON + (count - 1) * GAP;
    let lead = w.saturating_sub(row_w) / 2;
    let mut line = vec![seg(" ".repeat(lead), Tone::Text)];
    for (i, icon) in icons.iter().take(count).enumerate() {
        if i > 0 { line.push(seg(" ".repeat(GAP), Tone::Text)); }
        let padding = ICON.saturating_sub(icon.width());
        let left = padding / 2;
        line.push(seg(" ".repeat(left), Tone::Text));
        line.push(seg(*icon, tones[i]));
        line.push(seg(" ".repeat(padding - left), Tone::Text));
        let x = PAD + lead + i * (ICON + GAP);
        body.hits.push(Hit { row: start, x0: x.saturating_sub(1), x1: x + ICON + 1, action: [Action::VoiceMute, Action::VoiceDeaf, Action::VoiceOpen, Action::Screenshot][i].clone() });
    }
    body.lines.push(line);
    body.glued = true;
}

fn dash(v: &str) -> String { if v.is_empty() { "—".into() } else { v.into() } }

// ---- 1:MAIN ---------------------------------------------------------------------------
fn status_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let mut c = Card::new("status", mark("Status", PinkBright), w);
    c.extra = vec![(seg("▸", Dim), Some(Action::ModelPicker))];
    c.keyed("Project", vec![seg(short_path(&s.cwd, w.saturating_sub(4 + 8)), Text)]);
    let mut branch = vec![seg(format!("⎇ {}", if s.git.branch.is_empty() { "no-git" } else { &s.git.branch }), Text)];
    if let Some(n) = s.git.dirty.filter(|n| *n > 0) { branch.push(seg(format!(" ±{n}"), Yellow)); }
    c.keyed("Branch", branch);
    // Model (S10): its own section, spaced capitals like the reference image, `◆ THINKING ◆` below.
    c.divider(); model_section(&mut c, s); c.divider();
    let (icon, tone) = match s.status.as_str() { "working" => ("●", Cyan), "idle" => ("○", Muted), "blocked" => ("!", Coral), "done" => ("✓", Mint), _ => ("?", Dim) };
    let place = if s.label.is_empty() { dash(&s.workspace) } else { s.label.clone() };
    c.row(vec![seg(format!("{icon} {}", s.status), tone)], vec![seg(place, Muted)]);
    let (dot, word, tone) = if s.connected { ("●", "conectado", Mint) } else { ("○", "desconectado", Coral) };
    c.row(vec![seg("herdr ", Muted), seg(dot, tone), seg(format!(" {}", s.pane), Muted)], vec![seg(word, tone)]);
    c
}
fn model_section(c: &mut Card, s: &State) {
    use Tone::*;
    let inner = c.w.saturating_sub(4);
    let center = |c: &mut Card, g: Seg| { let g = fit(vec![g], inner); let pad = inner.saturating_sub(width_of(&g)) / 2; let mut l = vec![seg(" ".repeat(pad), Text)]; l.extend(g); c.row(l, vec![]); c.on_click(Action::ModelPicker); };
    // Before the first assistant turn the session has no model yet: use the host defaults (settings.json), like gs2.
    let host = s.roster.as_ref().filter(|_| s.session.model.is_empty());
    let (model, thinking) = match host { Some(r) => (r.model.as_str(), r.thinking.as_str()), None => (s.session.model.as_str(), s.session.thinking.as_str()) };
    let model = model.rsplit('/').next().unwrap_or_default().to_uppercase();
    if model.is_empty() { center(c, seg("sin modelo", Dim)); return }
    let spaced = model.chars().map(String::from).collect::<Vec<_>>().join(" ");
    if spaced.width() <= inner { center(c, bold(spaced, PinkBright)); } else { for part in wrap(&model, inner) { center(c, bold(part, PinkBright)); } }
    if !thinking.is_empty() && thinking != "off" { center(c, seg(format!("◆ {} ◆", thinking.to_uppercase()), Gold)); }
}
fn context_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let t = &s.session; let mut c = Card::new("context", mark("Context", PinkBright), w);
    let pct = t.limit.map(|n| (100.0 * t.used as f64 / n.max(1) as f64).clamp(0.0, 100.0));
    let (label, tone) = ctx_band(pct.unwrap_or(0.0));
    let limit = t.limit.map_or("—".into(), fmt_tokens);
    let right = match pct { Some(p) => bold(format!("{label} {}%", if p > 0.0 && p < 10.0 { format!("{p:.1}") } else { format!("{}", p.round()) }), tone), None => seg("sin límite", Dim) };
    c.row(vec![bold(fmt_tokens(t.used), tone), seg(" / ", Muted), seg(limit, Muted), seg(" tokens", Muted)], vec![right]);
    let cells = w.saturating_sub(4); let filled = ((pct.unwrap_or(0.0) / 100.0) * cells as f64).round() as usize;
    c.row(vec![seg("▰".repeat(filled.min(cells)), tone), seg("▱".repeat(cells - filled.min(cells)), Dim)], vec![]);
    let cost = vec![seg("Cost ", Gold), seg(fmt_cost(t.cost), Text)];
    let io = |sep: &str| vec![seg("▲ ", Muted), seg(fmt_tokens(t.input + t.read), Text), seg(format!(" in{sep}·{sep}"), Muted), seg("▼ ", Muted), seg(fmt_tokens(t.output), Text), seg(" out", Muted)];
    // Double-spaced like the reference when it fits next to the cost, single-spaced otherwise.
    let wide = io("  "); let left = if width_of(&wide) + width_of(&cost) < w.saturating_sub(4) { wide } else { io(" ") };
    c.row(left, cost);
    c
}

/// Word wrap like the reference `wrapWords`; a word wider than the row is cut char by char.
fn wrap(text: &str, w: usize) -> Vec<String> {
    let w = w.max(1); let mut out: Vec<String> = Vec::new(); let mut cur = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_string();
        if !cur.is_empty() && cur.width() + 1 + word.width() <= w { cur.push(' '); cur.push_str(&word); continue }
        if !cur.is_empty() { out.push(std::mem::take(&mut cur)); }
        while word.width() > w { let head = truncate(&word, w); let head = if head.is_empty() { word.chars().next().unwrap().to_string() } else { head }; word = word[head.len()..].to_string(); out.push(head); }
        cur = word;
    }
    if !cur.is_empty() || out.is_empty() { out.push(cur); }
    out
}
fn todos_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let todos = &s.session.todos; let done = todos.iter().filter(|t| t.status == TodoStatus::Done).count();
    let mut c = Card::new("todos", [mark("Todos", PinkBright), vec![seg(format!(" · {done} of {}", todos.len()), Text)]].concat(), w);
    if todos.is_empty() { c.row(vec![seg("sin tareas en esta sesión", Dim)], vec![]); }
    let inner = w.saturating_sub(4);
    for t in todos {
        let (glyph, gt, tt) = match t.status { TodoStatus::Done => ("✓", Mint, Dim), TodoStatus::InProgress => ("◉", Gold, PinkBright), TodoStatus::Pending => ("○", Dim, Text) };
        for (i, l) in wrap(&t.title, inner.saturating_sub(2)).into_iter().enumerate() {
            c.row(vec![if i == 0 { seg(format!("{glyph} "), gt) } else { seg("  ", Text) }, seg(l, tt)], vec![]);
        }
        if let (Some(n), TodoStatus::InProgress) = (&t.note, t.status) {
            for (i, l) in wrap(n, inner.saturating_sub(4)).into_iter().enumerate() { c.row(vec![seg(if i == 0 { "  ↳ " } else { "    " }, Dim), seg(l, Dim)], vec![]); }
        }
    }
    c
}

// ---- 2:GIT ----------------------------------------------------------------------------
fn changes_card(s: &State, w: usize) -> Card {
    let mut c=Card::new("changes",mark("Cambios",Tone::Gold),w);
    let message=s.ui.editing_commit_message.as_ref().unwrap_or(&s.ui.commit_message);
    c.row(vec![seg("mensaje: ",Tone::Gold),seg(if message.is_empty() {"escribir…"} else {message},Tone::Text)],vec![]);
    c.on_click(Action::EditCommitMessage);
    if s.git.changes.staged.is_empty() { c.row(vec![seg("faltan archivos staged para commit",Tone::Dim)],vec![]); }
    for (title, entries, staged) in [("staged",&s.git.changes.staged,true),("sin stage",&s.git.changes.unstaged,false)] {
        if entries.is_empty() {continue}
        c.row(vec![seg(title,Tone::Gold)],vec![]);
        let sensitive_row=entries.iter().any(|e|crate::core::sensitive(&e.path));
        for entry in entries {
            let letter=if staged {entry.index} else {entry.worktree};
            let path=entry.path.replace(|ch:char|ch.is_control()," ");
            // The verb cell is the click target: the rail stages/unstages, nothing else.
            let verb=if crate::core::sensitive(&path) {"rail"} else if staged {"unstage"} else {"stage"};
            let mut right=vec![seg(format!("[{verb}]"),if verb=="rail" {Tone::Dim} else {Tone::Cyan})];
            if let Some((a,d))=entry.numstat {right.insert(0,seg(format!("+{a} −{d} "),Tone::Mint));}
            let room=w.saturating_sub(7+width_of(&right));
            let path=if path.width()>room {
                let name=path.rsplit('/').next().unwrap_or(&path);
                if name.width()+2<=room {format!("…/{name}")} else {truncate(&path,room)}
            } else {path};
            c.row(vec![seg(format!("{letter}  {path}"),Tone::Text)],right);
            if !crate::core::sensitive(&entry.path) {
                c.on_click(if staged {Action::Unstage(entry.path.clone())} else {Action::Stage(entry.path.clone())});
            }
        }
        if entries.len()>1 {
            let (label,action)=if staged {("[unstage all]",Action::UnstageAll)} else {("[stage all]",Action::StageAll)};
            let _=sensitive_row;
            c.row(vec![],vec![seg(label,Tone::Cyan)]);
            c.on_click(action);
        }
    }
    // The commit itself: only offered when it can succeed, and it says what is missing otherwise.
    let ready=!s.git.changes.staged.is_empty() && !s.ui.commit_message.trim().is_empty();
    c.divider();
    if ready {
        c.row(vec![seg("[✓ commit]",Tone::Mint)],vec![seg(format!("{} staged",s.git.changes.staged.len()),Tone::Dim)]);
        c.on_click(Action::CommitNow);
    } else {
        let why=if s.git.changes.staged.is_empty() {"sin archivos staged"} else {"sin mensaje"};
        c.row(vec![seg("[✓ commit]",Tone::Dim)],vec![seg(why,Tone::Dim)]);
    }
    // What the last write did: git's own words, verbatim.
    if let Some((ok,text))=s.git_notice.as_ref() {
        let tone=if *ok {Tone::Mint} else {Tone::Coral};
        // Git's own words, all of them: a hook explains itself in whatever lines it needs. A very
        // long error keeps its tail visible by naming how many lines the card is not drawing.
        let lines:Vec<&str>=text.lines().collect();
        for (i,line) in lines.iter().enumerate().take(12) {c.row(vec![seg(if i==0 {format!("{} ",if *ok {"✔"} else {"✖"})} else {"  ".into()},tone),seg(line.to_string(),tone)],vec![]);}
        if lines.len()>12 {c.row(vec![seg(format!("  … {} líneas más",lines.len()-12),Tone::Dim)],vec![]);}
    }
    c
}
/// `formatGitStatusBadges`: `✔ clean`, or conflicts · mod · staged · untracked.
fn status_badge(g: &Git) -> Vec<Seg> {
    use Tone::*;
    if g.dirty.is_none() { return vec![seg("—", Dim)] }
    let c = &g.counts; if c.clean() { return vec![seg("✔ clean", Mint)] }
    let mut out = Vec::new();
    for (n, text, tone) in [(c.conflicts, format!("✖ {} conflict", c.conflicts), Coral), (c.modified, format!("● {} mod", c.modified), Yellow),
        (c.staged, format!("+{} staged", c.staged), Mint), (c.untracked, format!("?{} untracked", c.untracked), Secondary)] {
        if n == 0 { continue }
        if !out.is_empty() { out.push(seg(" · ", Muted)); }
        out.push(seg(text, tone));
    }
    out
}
/// Graph glyphs: nodes pink, edges blue (`colorizeGitGraphLine`).
fn edges(g: &str) -> Vec<Seg> {
    let mut v: Vec<Seg> = Vec::new();
    for ch in g.chars() {
        let t = match ch { '*' => Tone::Pink, ' ' => Tone::Text, _ => Tone::Blue };
        match v.last_mut() { Some(last) if last.tone == t => last.text.push(ch), _ => v.push(seg(ch.to_string(), t)) }
    }
    v
}
fn graph_line(line: &str) -> Vec<Seg> {
    use Tone::*;
    let Some((g, hash, refs, subject)) = split_graph(line) else { return edges(line) };
    let mut v = edges(g); v.push(seg(hash, Yellow));
    if let Some(r) = refs {
        v.push(seg(" ", Text)); v.push(seg("(", Muted));
        for (i, item) in r.split(',').map(str::trim).enumerate() {
            if i > 0 { v.push(seg(", ", Muted)); }
            let t = if item.contains("HEAD") { Mint } else if item.contains("origin") || item.contains("upstream") { Secondary } else { Gold };
            v.push(seg(item, t));
        }
        v.push(seg(")", Muted));
    }
    if !subject.is_empty() { v.push(seg(format!(" {subject}"), Text)); }
    v
}
fn git_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let (g, ui) = (&s.git, &s.ui);
    let mut c = Card::new("git", vec![seg("⎇ git graph", Gold)], w);
    let compact = w < 38;
    let label = |v: GitView, long: &'static str| -> (Seg, Option<Action>) {
        let text = if compact { &long[..1] } else { long };
        (if ui.git_view == v { bold(text, Pink) } else { seg(text, Dim) }, Some(Action::GitView(v)))
    };
    c.extra = vec![(seg("[ ", Dim), None), label(GitView::Graph, "Grafo"), (seg(" | ", Dim), None), label(GitView::Branches, "Ramas"), (seg(" ]", Dim), None)];
    if let Some(e) = &g.error { c.row(vec![seg(e.clone(), Coral)], vec![]); }
    match ui.git_view {
        GitView::Graph => {
            c.row(vec![], vec![]); c.row(vec![seg(dash(&g.branch), Text)], status_badge(g)); c.row(vec![], vec![]);
            // A window of 25 rows and the wheel walks back: slicing rows (not commits) keeps the ASCII
            // connectors aligned with the commits they join, and the title says which slice is on screen.
            let total = g.graph.len();
            let max_scroll = total.saturating_sub(GRAPH_ROWS);
            let scroll = ui.git_scroll.min(max_scroll);
            let end = (scroll + GRAPH_ROWS).min(total);

            let visible = g.graph.get(scroll..end).unwrap_or(&[]);
            for l in visible {
                // The hash a row draws is the only thing a click can name, so the row itself is the hit.
                let hash = split_graph(l).map(|(_, h, _, _)| h.to_string());
                let open = hash.as_deref().is_some_and(|h| ui.git_commit.as_deref() == Some(h));
                let mut segs = graph_line(l);
                if open { segs.insert(0, seg("▸ ", PinkBright)); }
                c.row(segs, vec![]);
                if let Some(h) = hash { c.on_click(Action::Commit(h)); }
            }
            // Which slice is on screen. A row of its own, because the title already carries the switch and
            // the rail is 48 columns wide.
            if total > 0 {
                c.row(vec![seg(if total > GRAPH_ROWS { "↕ " } else { "  " }, Dim), seg(format!("{}-{end}/{total}", scroll + 1), Dim)], vec![]);
            }
            // The whole card takes the wheel, like the MEDIA lists: the hand is on it.
            c.list = Some((scroll, max_scroll));
            // One page at a time: the row says how many more there are, and reaching the end of the body asks
            // for the same page on its own. Fewer commits than the page means the repository ended.
            if g.commits.len() >= g.log_limit && g.log_limit < crate::sources::LOG_MAX {
                let next = (g.log_limit * 2).min(crate::sources::LOG_MAX);
                c.row(vec![seg("⋯ ", Pink), seg(format!("cargar {} más", next - g.log_limit), Dim)], vec![]);
                c.on_click(Action::MoreCommits(next));
            }
            if g.added + g.deleted > 0 {
                let n = g.counts.modified + g.counts.staged;
                c.row(vec![], vec![]);
                c.row(vec![seg("✎ ", Pink), seg(format!("{n} {} · ", if n == 1 { "file" } else { "files" }), Text), seg(format!("+{}", g.added), Mint), seg(" ", Text), seg(format!("−{}", g.deleted), Coral)], vec![]);
            }
        }
        GitView::Branches => {
            let head = vec![seg(format!("● {}", g.branches.len()), Yellow), seg(" ramas en repo", Dim)];
            let badge = status_badge(g); let fits = width_of(&head) + 1 + width_of(&badge) <= w.saturating_sub(4);
            c.row(head, if fits { badge } else { vec![] });
            for b in &g.branches {
                let sel = ui.git_branch.as_deref() == Some(b.name.as_str());
                let name = match b.name.strip_prefix("origin/") { Some(n) if b.remote => format!("rem/{n}"), _ => b.name.clone() };
                let (icon, it, nt) = if b.current { ("⎇ ", Mint, Mint) } else if sel { ("▶ ", Pink, Pink) } else { ("● ", Dim, if b.remote { Secondary } else { Cyan }) };
                let mut right: Vec<Seg> = Vec::new();
                let add = |r: &mut Vec<Seg>, g: Seg| { if !r.is_empty() { r.push(seg(" ", Text)); } r.push(g); };
                if b.current { add(&mut right, seg("actual", Mint)); } else if sel { add(&mut right, seg("diff", Pink)); } else {
                    if b.ahead > 0 { add(&mut right, seg(format!("↑{}", b.ahead), Yellow)); }
                    if b.behind > 0 { add(&mut right, seg(format!("↓{}", b.behind), Coral)); }
                    if b.gone { add(&mut right, seg("gone", Coral)); }
                    if right.is_empty() && !b.date.is_empty() { add(&mut right, seg(b.date.clone(), Dim)); }
                }
                c.row(vec![seg(icon, it), seg(name, nt)], right);
                if !b.current { c.on_click(Action::Branch(Some(b.name.clone()))); }
            }
        }
    }
    c
}
/// Diff preview rows under the selected file: hunks only (headers dropped), capped at DIFF_ROWS.
const DIFF_ROWS: usize = 80;
fn diff_rows(c: &mut Card, s: &State, path: &str) {
    use Tone::*;
    let (d, ui) = (&s.detail, &s.ui);
    let note = |c: &mut Card, t: &str| c.row(vec![seg(format!("  {t}"), Dim)], vec![]);
    if sensitive(path) { return note(c, "contenido oculto · posible secreto") }
    if d.file != ui.git_file || d.branch != ui.git_branch || d.commit != ui.git_commit { return note(c, "cargando diff…") }
    let lines = &d.diff[d.diff.iter().position(|l| l.starts_with("@@")).unwrap_or(0)..];
    if lines.is_empty() { return note(c, "sin cambios de texto") }
    for l in lines.iter().take(DIFF_ROWS) {
        let t = if l.starts_with("@@") { Blue } else if l.starts_with('+') { Mint } else if l.starts_with('-') { Coral } else { Muted };
        c.row(vec![seg("  ", Text), seg(l.replace('\t', "    ").replace('\r', ""), t)], vec![]);
    }
    let more = lines.len().saturating_sub(DIFF_ROWS) + d.more;
    if more > 0 { note(c, &format!("… {more} líneas más")); }
}
fn file_rows(c: &mut Card, s: &State, files: &[FileChange]) {
    use Tone::*;
    for f in files {
        let (b, t) = match f.kind { FileKind::Modified => ("M", Yellow), FileKind::Added => ("A", Mint), FileKind::Deleted => ("D", Coral), FileKind::Untracked => ("?", Pink), FileKind::Conflict => ("U", Coral) };
        let (dir, name) = f.path.rfind('/').map_or(("", f.path.as_str()), |i| (&f.path[..=i], &f.path[i + 1..]));
        let mut right = vec![seg("[", Dim), seg("diff", Pink), seg("]", Dim)];
        if f.added > 0 { right.extend([seg(" ", Text), seg(format!("+{}", f.added), Mint)]); }
        if f.deleted > 0 { right.extend([seg(" ", Text), seg(format!("−{}", f.deleted), Coral)]); }
        // Too long: the directory shrinks to `…/` so the file name stays readable.
        let room = c.w.saturating_sub(4 + width_of(&right) + 1 + 3);
        let dir = if !dir.is_empty() && dir.width() + name.width() > room { "…/" } else { dir };
        let mut left = vec![seg(b, t), seg("  ", Text)]; if !dir.is_empty() { left.push(seg(dir, Dim)); } left.push(seg(name, Cyan));
        c.row(left, right); c.on_click(Action::File(f.path.clone()));
        if s.ui.git_file.as_deref() == Some(f.path.as_str()) { diff_rows(c, s, &f.path); }
    }
}
/// The text the rail hands to its own window: what a commit touched, then the diff of the file that is
/// open, in 24-bit SGR so the window can colour it. The window is a text view, not a second copy of the
/// rail's layout, so colour travels in the text itself.
fn sgr(tone: Tone, text: &str) -> String {
    let (r, g, b) = tone.rgb();
    format!("\u{1b}[38;2;{r};{g};{b}m{text}\u{1b}[0m")
}

/// One cell of the two-panel view: the text, then as many spaces as it needs to reach `width`, so the
/// separator lands in the same column on every row. Colour covers the padding too, which is invisible.
fn cell(tone: Tone, text: &str, width: usize) -> String {
    let pad = " ".repeat(width.saturating_sub(text.width()));
    format!("{}{pad}", sgr(tone, text))
}

/// Rows of the comparison: what the file had on the left, what it has now on the right. A hunk header
/// spans both, a deleted line has nothing on the right, an added line nothing on the left, and a context
/// line is the same text twice — the pairing a two-panel diff application shows.
fn pairs(diff: &[String]) -> Vec<(Option<String>, Option<String>)> {
    let mut rows = Vec::new();
    let mut i = 0;
    let mut in_hunk = false;
    while i < diff.len() {
        let line = &diff[i];
        // `git show` opens with its own header (`diff --git`, `index`, `---`, `+++`, modes, renames).
        // It says nothing about the change and would print twice, once per column, before every file.
        if !in_hunk && ["diff --git ", "index ", "--- ", "+++ ", "new file mode ", "deleted file mode ", "old mode ", "new mode ", "similarity index ", "rename from ", "rename to ", "Binary files "].iter().any(|p| line.starts_with(p)) { i += 1; continue }
        if line.starts_with("@@") { in_hunk = true }
        let removed = line.starts_with('-') && !line.starts_with("---");
        let added = line.starts_with('+') && !line.starts_with("+++");
        if removed {
            let start = i;
            while i < diff.len() && diff[i].starts_with('-') && !diff[i].starts_with("---") { i += 1 }
            let gone: Vec<String> = diff[start..i].to_vec();
            let begin = i;
            while i < diff.len() && diff[i].starts_with('+') && !diff[i].starts_with("+++") { i += 1 }
            let now: Vec<String> = diff[begin..i].to_vec();
            // The classic pairing: the first removed line faces the first added one, and whatever is left
            // over faces an empty cell, so the rows stay aligned.
            for k in 0..gone.len().max(now.len()) {
                rows.push((gone.get(k).cloned(), now.get(k).cloned()));
            }
        } else if added {
            rows.push((None, Some(line.clone())));
            i += 1;
        } else if line.starts_with("@@") {
            rows.push((Some(line.clone()), None));
            i += 1;
        } else {
            rows.push((Some(line.clone()), Some(line.clone())));
            i += 1;
        }
    }
    rows
}

/// Break a line into cells of at most `width` characters. A long line is folded inside its own column
/// instead of running through the separator into the other one, which is what made the left side of a
/// long change print on top of the right.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut line = String::new();
    for ch in text.chars() {
        if line.chars().count() >= width && width > 0 { out.push(std::mem::take(&mut line)); }
        line.push(ch);
    }
    out.push(line);
    out
}

/// A paragraph wrapped on spaces: a commit message is prose, and cutting it mid-word reads worse than a
/// shorter line. Only a token wider than the whole column is cut, because nothing else would fit it.
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let count = word.chars().count();
        if width == 0 { out.push(word.to_string()); continue }
        if count > width {
            if !line.is_empty() { out.push(std::mem::take(&mut line)) }
            let chars: Vec<char> = word.chars().collect();
            for chunk in chars.chunks(width) { out.push(chunk.iter().collect()) }
            continue
        }
        let grown = line.chars().count() + usize::from(!line.is_empty()) + count;
        if grown > width { out.push(std::mem::take(&mut line)) }
        if !line.is_empty() { line.push(' ') }
        line.push_str(word);
    }
    if !line.is_empty() || out.is_empty() { out.push(line) }
    out
}

/// The text the rail hands to its own window: the commit, the files it touched, and then a two-panel
/// comparison of the file that is open — the old lines on the left, the new ones on the right, each
/// side wrapped inside its own column so a long line never crosses over.
pub fn panel_text(d: &Detail, hash: &str) -> String {
    use Tone::*;
    // Half of the window at its default width (70% of a 1920 px screen, 12 pt monospace ≈ 185 columns):
    // both panels get the same size, and each one wraps inside it.
    const COL: usize = 80;
    let mut out = format!("{} {} {} {}\n\n", sgr(Gold, &format!("◆ commit {hash}")), sgr(Muted, "["), sgr(Coral, "✕"), sgr(Muted, "] [-]"));
    // The commit's own words. The graph only ever printed the subject; the window is where the whole
    // explanation fits, and it is wrapped because this text view does not wrap by itself (a diff must not).
    if !d.author.is_empty() || !d.date.is_empty() {
        out.push_str(&format!("{}\n\n", sgr(Muted, &format!("{} · {}", d.author, d.date))));
    }
    let message = d.message.trim();
    if !message.is_empty() {
        for line in message.lines() {
            if line.trim().is_empty() { out.push('\n'); continue }
            for chunk in wrap_words(line, 100) { out.push_str(&sgr(Text, &chunk)); out.push('\n'); }
        }
        out.push('\n');
    }
    if d.files.is_empty() { out.push_str(&sgr(Dim, "  sin archivos")); out.push('\n'); }
    for f in &d.files {
        let (letter, tone) = match f.kind { FileKind::Modified => ("M", Yellow), FileKind::Added => ("A", Mint), FileKind::Deleted => ("D", Coral), FileKind::Untracked => ("?", Pink), FileKind::Conflict => ("U", Coral) };
        out.push_str(&format!("{} {} {} {} {}\n",
            sgr(tone, letter),
            sgr(Cyan, &f.path),
            sgr(Pink, "[diff]"),
            sgr(Mint, &format!("+{}", f.added)),
            sgr(Coral, &format!("-{}", f.deleted))));
    }
    if d.added + d.deleted > 0 {
        out.push_str(&format!("  {} {}\n", sgr(Mint, &format!("+{}", d.added)), sgr(Coral, &format!("-{}", d.deleted))));
    }
    if !d.diff.is_empty() {
        out.push('\n');
        out.push_str(&format!("{}│{}\n", cell(Dim, "  antes", COL), sgr(Dim, "  ahora")));
        for (old, new) in pairs(&d.diff) {
            let hunk = old.as_deref().is_some_and(|l| l.starts_with("@@"));
            if hunk {
                // Inside its own column, never across: a header that spans both panels reads as text
                // overflowing into the other side, which is exactly what the two panels avoid.
                let text = old.unwrap_or_default().replace('\t', "    ");
                for chunk in wrap_text(&text, COL) {
                    out.push_str(&format!("{}{}\n", cell(Blue, &chunk, COL), sgr(Dim, "│")));
                }
                continue
            }
            let gone = old.clone().unwrap_or_default().replace('\t', "    ");
            let now = new.clone().unwrap_or_default().replace('\t', "    ");
            let left_tone = if gone.starts_with('-') { Coral } else { Text };
            let right_tone = if now.starts_with('+') { Mint } else { Text };
            let left = wrap_text(&gone, COL);
            let right = wrap_text(&now, COL);
            for k in 0..left.len().max(right.len()) {
                let l = left.get(k).map(String::as_str).unwrap_or("");
                let r = right.get(k).map(String::as_str).unwrap_or("");
                out.push_str(&format!("{}{}{}\n", cell(left_tone, l, COL), sgr(Dim, "│"), if r.is_empty() { String::new() } else { sgr(right_tone, r) }));
            }
        }
    }
    out
}

/// The window's text for a TREE click: what the file is, where it came from, and the file itself. Colour
/// travels in the text, exactly like the diff's, so the window parses the same SGR it already parses, and
/// the `✕` in the header is the close button it already knows.
pub fn preview_text(p: &Preview) -> String {
    use Tone::*;
    let head = |title: &str| format!("{} {} {} {}\n\n", sgr(Gold, title), sgr(Muted, "["), sgr(Coral, "✕"), sgr(Muted, "] [-]"));
    match p {
        // Never reached in practice (an image is drawn by the window, not written here), kept so the
        // function is total and a caller that got it wrong says so instead of drawing nothing.
        Preview::Image { path, .. } => format!("{}{}", head("🖼 archivo"), sgr(Cyan, path)),
        // Same: markdown goes to the window as a document, never as the text this function builds.
        Preview::Markdown { path, .. } => format!("{}{}", head("◆ markdown"), sgr(Cyan, path)),
        // Same: source code goes to the window already coloured by its grammar.
        Preview::Code { path, .. } => format!("{}{}", head("◆ código"), sgr(Cyan, path)),
        Preview::Refused { path, note } => format!("{}{}\n\n{}\n", head("🗒 archivo"), sgr(Cyan, path), sgr(Coral, note)),
        Preview::Text { path, body, cut } => {
            // No wrapping: the window scrolls sideways, so every line keeps the columns it was written with.
            let mut out = format!("{}{}\n\n", head("🗒 archivo"), sgr(Cyan, path));
            for line in body.lines() { out.push_str(&line.replace('\t', "    ")); out.push('\n'); }
            if *cut { out.push_str(&format!("\n{}\n", sgr(Dim, &format!("… sólo los primeros {} KiB", PREVIEW_MAX / 1024)))); }
            out
        }
    }
}

/// What the rail's own window should show, with the key that says "same answer as last time": the diff of
/// the commit open in GIT, or the file picked in TREE. One window serves both tabs, and the rail has 50
/// columns, so neither a diff nor an image fits in them.
pub fn panel_show(s: &State) -> Option<(String, usize, PanelCmd)> {
    match s.ui.tab {
        Tab::Git => s.ui.git_commit.clone().filter(|h| s.detail.commit.as_deref() == Some(h.as_str())).map(|hash| {
            let text = panel_text(&s.detail, &hash);
            let n = text.len();
            (hash.clone(), n, PanelCmd::Show { hash, text, geometry: None })
        }),
        // Only the answer for the file still picked: clicks may outrun a read.
        Tab::Tree => s.preview.as_ref().filter(|p| s.ui.tree_pick.as_deref() == Some(p.path())).map(|p| {
            let key = format!("file:{}", p.path());
            match p {
                // The window reads and draws the image itself: its bytes never cross the rail.
                Preview::Image { file, .. } => (key.clone(), 0, PanelCmd::Image { key, file: file.clone() }),
                // Markdown crosses as source and the window renders it: headings and tables are the point.
                Preview::Markdown { path, body, cut } => (key.clone(), body.len(), PanelCmd::Markdown { key, text: body.clone(), title: path.clone(), cut: *cut }),
                // Source code crosses as the window's own SGR, exactly like a diff.
                Preview::Code { body, .. } => { let text = code_text(p); let n = text.len(); (key.clone(), n, PanelCmd::Show { hash: key, text, geometry: None }) }
                other => { let text = preview_text(other); let n = text.len(); (key.clone(), n, PanelCmd::Show { hash: key, text, geometry: None }) }
            }
        }),
        _ => None,
    }
}

/// A source file as the window shows it: which language it is, then the file coloured by its grammar. The
/// SGR comes from the same place the diff's does, so the window needs no code of its own for this.
pub fn code_text(p: &Preview) -> String {
    use Tone::*;
    let Preview::Code { path, body, cut, lang } = p else { return preview_text(p) };
    let mut out = format!("{} {} {} {} {}\n\n", sgr(Gold, &format!("◆ {lang}")), sgr(Muted, "["), sgr(Coral, "✕"), sgr(Muted, "] [-]"), sgr(Cyan, path));
    out.push_str(&syntax::sgr(path, body));
    if *cut { out.push_str(&format!("\n{}\n", sgr(Dim, &format!("… sólo los primeros {} KiB", PREVIEW_MAX / 1024)))); }
    out
}

/// What a commit touched: the files it changed, and the diff of the one that is open. A commit is read
/// from what the graph printed, so the card says "cargando…" until the answer for that exact hash lands.
fn commit_card(s: &State, w: usize) -> Option<Card> {
    use Tone::*;
    let hash = s.ui.git_commit.as_ref()?;
    let d = &s.detail;
    let close = Some(Action::Commit(hash.clone()));
    let mut c = Card::new("commit", vec![seg("◆ commit ", Gold), seg(hash.clone(), Yellow)], w);
    c.extra = vec![(seg("[ ", Dim), close.clone()), (seg("✕", Coral), close.clone()), (seg(" ]", Dim), close)];
    if d.commit.as_deref() != Some(hash.as_str()) { c.row(vec![seg("  cargando…", Dim)], vec![]); return Some(c) }
    if let Some(e) = &d.error { c.row(vec![seg(format!("  {e}"), Coral)], vec![]); }
    if d.files.is_empty() && d.error.is_none() { c.row(vec![seg("  sin archivos", Dim)], vec![]); }
    for f in &d.files {
        let (b, t) = match f.kind { FileKind::Modified => ("M", Yellow), FileKind::Added => ("A", Mint), FileKind::Deleted => ("D", Coral), FileKind::Untracked => ("?", Pink), FileKind::Conflict => ("U", Coral) };
        let mut right = vec![seg("[", Dim), seg("diff", Pink), seg("]", Dim)];
        if f.added > 0 { right.extend([seg(" ", Text), seg(format!("+{}", f.added), Mint)]); }
        if f.deleted > 0 { right.extend([seg(" ", Text), seg(format!("−{}", f.deleted), Coral)]); }
        c.row(vec![seg(b, t), seg("  ", Text), seg(f.path.clone(), Cyan)], right);
        c.on_click(Action::File(f.path.clone()));
        if s.ui.git_file.as_deref() == Some(f.path.as_str()) { diff_rows(&mut c, s, &f.path); }
    }
    if d.added + d.deleted > 0 {
        c.row(vec![seg(format!("  +{}", d.added), Mint), seg(" ", Text), seg(format!("−{}", d.deleted), Coral)], vec![]);
    }
    Some(c)
}

/// Working tree changes, or (a branch selected in Ramas) that branch's diff against HEAD.
fn worktree_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let mut c = Card::new("worktree", vec![seg("📁 Working Tree", Gold)], w);
    let (files, added, deleted, branch) = match &s.ui.git_branch {
        Some(b) => {
            c.title = vec![seg("📁", Pink), seg(" Diff", Gold), seg(" · ", Dim), seg(b.clone(), Cyan)];
            let close = Some(Action::Branch(None)); c.extra = vec![(seg("[ ", Dim), close.clone()), (seg("✕", Coral), close.clone()), (seg(" ]", Dim), close)];
            if s.detail.branch.as_ref() != Some(b) { c.row(vec![seg("cargando…", Dim)], vec![]); return c }
            if let Some(e) = &s.detail.error { c.row(vec![seg(e.clone(), Coral)], vec![]); return c }
            (&s.detail.files, s.detail.added, s.detail.deleted, true)
        }
        None => (&s.git.files, s.git.added, s.git.deleted, false),
    };
    if files.is_empty() {
        let l = if branch { vec![seg("✔ ", Text), seg("Sin diferencias con HEAD", Mint)] } else { vec![seg("✔ ", Text), seg("Clean", Mint), seg(" · ", Dim), seg("Sin cambios pendientes", Dim)] };
        c.row(l, vec![]); return c
    }
    let n = files.len();
    let noun = match (branch, n == 1) { (true, true) => "archivo", (true, false) => "archivos", (false, true) => "archivo modificado", (false, false) => "archivos modificados" };
    let deltas = if added + deleted > 0 { vec![seg(format!("+{added}"), Mint), seg("  ", Text), seg(format!("−{deleted}"), Coral)] } else { vec![] };
    c.row(vec![seg(format!("● {n}"), Yellow), seg(format!(" {noun}"), Dim)], deltas);
    c.row(vec![seg("click: ver diff", Dim)], vec![]);
    c.divider();
    file_rows(&mut c, s, files);
    c
}
fn pr_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let mut c = Card::new("pr", mark("Pull requests", PinkBright), w);
    for l in &s.pr { let tone = if l.contains("error") { Coral } else if l.starts_with('#') { Cyan } else { Text }; c.row(vec![seg(l.clone(), tone)], vec![]); }
    c
}

// ---- 5:TREE ---------------------------------------------------------------------------
/// Reference `getFileIcon` (no Nerd Font; ⚙️ is replaced by the single-width-safe 🔧).
fn file_icon(name: &str) -> &'static str {
    let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "json" | "yaml" | "yml" | "toml" => "🔧", "md" | "txt" | "markdown" => "📝", "css" | "scss" | "sass" | "less" => "🎨",
        "html" | "htm" => "🌐", "png" | "jpg" | "jpeg" | "gif" | "svg" | "ico" | "webp" => "🖼", "sh" | "bash" | "zsh" => "💻", "lock" => "🔒", _ => "📄",
    }
}
const TREE_ROWS: usize = 300;
fn tree_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let mut c = Card::new("tree", mark("PROYECTO", Gold), w);
    let Some(t) = &s.tree else { c.row(vec![seg("Escaneando…", Dim)], vec![]); return c };
    if !t.project.is_empty() { c.title.extend([seg(" · ", Dim), seg(t.project.clone(), Cyan)]); }
    if !s.ui.tree_open.is_empty() { c.extra = vec![(seg("[ ", Dim), Some(Action::Fold)), (seg("⊟", Pink), Some(Action::Fold)), (seg(" ]", Dim), Some(Action::Fold))]; }
    if let Some(e) = &t.error { c.row(vec![seg(e.clone(), Coral)], vec![]); return c }
    if t.nodes.is_empty() { c.row(vec![seg("Sin archivos en el proyecto", Dim), seg(" · ", Dim), seg(MARK, Purple)], vec![]); return c }
    // Git badges: porcelain paths are root-relative, like `TreeNode.rel`.
    let kind = |rel: &str| s.git.files.iter().find(|f| f.path == rel).map(|f| f.kind);
    let dirty = |rel: &str| s.git.files.iter().any(|f| f.path.len() > rel.len() && f.path.starts_with(rel) && f.path.as_bytes()[rel.len()] == b'/');
    let mut rows: Vec<(String, &TreeNode)> = Vec::new();
    fn walk<'a>(nodes: &'a [TreeNode], prefix: &str, open: &[String], out: &mut Vec<(String, &'a TreeNode)>) {
        for (i, n) in nodes.iter().enumerate() {
            let last = i + 1 == nodes.len();
            out.push((format!("{prefix}{}", if last { "└── " } else { "├── " }), n));
            if n.dir && open.contains(&n.rel) { walk(&n.children, &format!("{prefix}{}", if last { "    " } else { "│   " }), open, out); }
        }
    }
    walk(&t.nodes, "", &s.ui.tree_open, &mut rows);
    let more = rows.len().saturating_sub(TREE_ROWS);
    for (prefix, n) in rows.into_iter().take(TREE_ROWS) {
        let mut left = vec![seg(prefix, Dim)];
        if n.dir {
            let open = s.ui.tree_open.contains(&n.rel); let d = dirty(&n.rel);
            left.extend([seg(if open { "📂 " } else { "📁 " }, Text), seg(n.name.clone(), if d { Yellow } else { Cyan }), seg(format!(" ({} items)", n.count), Dim)]);
            c.row(left, vec![seg(if d { "●" } else { " " }, Yellow)]); c.on_click(Action::Dir(n.rel.clone()));
        } else {
            let base = match n.name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).as_deref() {
                Some("rs" | "ts" | "tsx" | "js" | "jsx" | "mjs") => Mint, Some("json" | "yaml" | "yml" | "toml") => Gold, _ => Text };
            let (badge, tone) = match kind(&n.rel) {
                Some(FileKind::Modified) => ("M", Yellow), Some(FileKind::Added) => ("A", Mint), Some(FileKind::Untracked) => ("?", Pink),
                Some(FileKind::Conflict) => ("U", Coral), Some(FileKind::Deleted) => ("D", Coral), None => (" ", base) };
            let name = if s.ui.tree_pick.as_deref() == Some(n.rel.as_str()) { bold(n.name.clone(), Pink) } else { seg(n.name.clone(), tone) };
            left.extend([seg(format!("{} ", file_icon(&n.name)), Text), name]);
            c.row(left, vec![seg(badge, tone)]);
            c.on_click(if kind(&n.rel).is_some() { Action::Inspect(n.rel.clone()) } else { Action::Pick(n.rel.clone()) });
        }
    }
    if more > 0 { c.row(vec![seg(format!("… {more} más"), Dim)], vec![]); }
    c
}

// ---- 3:PROF ---------------------------------------------------------------------------
/// The `/gentle:profiles` store card: one row per profile, the active one marked. It only appears
/// when there is a real choice (more than one profile), and a click only leaves the name selected
/// for the human — the HUD never writes the store nor applies the profile.
/// `⌁ Salud del ruteo`: what is broken in the configured routing, never what it should be. Every
/// warning names the exit and nothing here executes an action; a clean routing gets one row.
fn health_card(s: &State, w: usize) -> Option<Card> {
    use Tone::*;
    let r = s.roster.as_ref()?;
    // No readable store means no judge: "clean" would be a claim the rail cannot support.
    if r.store_missing {
        let mut c = Card::new("prof-health", [mark("Salud del ruteo", Gold), vec![seg(" · sin evaluar", Dim)]].concat(), w);
        c.row(vec![seg("◌ store de perfiles ilegible", Yellow)], vec![seg("no se puede evaluar", Dim)]);
        c.row(vec![seg("  ▸ revisá la ruta del store configurado", Dim)], vec![]);
        return Some(c);
    }
    let known = |name: &str| r.profiles.iter().any(|p| p.name == name);
    // Each issue is a compact row plus an optional exit row: at 36 columns the whole sentence
    // does not fit beside its remedy, and a truncated warning is not a warning.
    let mut issues: Vec<(Vec<Seg>, Vec<Seg>, Option<String>)> = Vec::new();
    // A pin layer that names a profile the store no longer has: invisible until now.
    for (layer, name) in &r.pin_stale {
        issues.push((
            vec![seg("▣ ", Cyan), seg(format!("pin {layer}: {name}"), Text)],
            vec![seg("no está en el store", Coral)],
            // `publish` republishes a declaration; it does not restore a missing profile, so the
            // exit is the one that actually fixes the orphan: recreate it, or drop the pin.
            Some(format!("recreá {name} en la pantalla de perfiles, o quitá el pin")),
        ));
    }
    // A session binding whose profile left the store renders an empty map otherwise.
    if let Some(Some(profile)) = s.profile_channel.as_ref() {
        let name = text_of(session_profile(profile), "name");
        if !name.is_empty() && !known(&name) {
            issues.push((
                vec![seg("◉ ", Gold), seg(format!("sesión: {name}"), Text)],
                vec![seg("no está en el store", Coral)],
                Some("elegí otro perfil en esta sesión".into()),
            ));
        }
    }
    // A role with no usable model is reported, never guessed from a sibling — and the roles
    // judged are the ones that actually resolve: the session profile's map when one is loaded,
    // the global assignments otherwise. Judging the global roster under a session profile would
    // warn about roles the session already resolves, or stay quiet about roles it loses.
    let mut missing: Vec<String> = Vec::new();
    match s.profile_channel.as_ref().and_then(|o| o.as_ref()) {
        Some(profile) => {
            let models = &session_profile(profile)["models"];
            if let Some(entries) = models.as_object() {
                for (role, route) in entries {
                    if role == "orchestrator" { continue }
                    if route["model"].as_str().unwrap_or("").is_empty() { missing.push(role.clone()); }
                }
            }
        }
        None => { for a in r.agents.iter().filter(|a| a.model.is_empty()) { missing.push(a.name.clone()); } }
    }
    for name in &missing {
        issues.push((
            // `rol <nombre>` keeps this row from reading like the roster row of the same role.
            vec![seg("• ", Salmon), seg(format!("rol {name}"), Salmon)],
            vec![seg("sin modelo", Coral)],
            // The literal command is the footer row's; this cell only names the exit.
            Some("abrir perfiles".into()),
        ));
    }
    let title = if issues.is_empty() { mark("Salud del ruteo", Mint) } else { [mark("Salud del ruteo", Gold), vec![seg(format!(" · {} aviso{}", issues.len(), if issues.len() == 1 {""} else {"s"}), Coral)]].concat() };
    let mut c = Card::new("prof-health", title, w);
    if issues.is_empty() {
        c.row(vec![seg("✔ ruteo consistente", Mint)], vec![seg("sin problemas", Dim)]);
        return Some(c);
    }
    for (left, right, hint) in issues {
        c.row(left, right);
        if let Some(hint) = hint { c.row(vec![seg(format!("  ▸ {hint}"), Dim)], vec![]); }
    }
    Some(c)
}
fn profile_store_card(s: &State, w: usize) -> Option<Card> {
    use Tone::*;
    let r = s.roster.as_ref()?;
    if r.profiles.len() < 2 { return None }
    let mut c = Card::new("profiles-store", mark("Perfiles", Cyan), w);
    // The session channel answer: None until the first reply, Some(None) when the channel is
    // down (the extension is not loaded), Some(profile) when the session has a binding.
    let session = s.profile_channel.as_ref().and_then(|o| o.as_ref());
    let session_name = session.map(|v| text_of(session_profile(v), "name")).filter(|n| !n.is_empty());
    for p in &r.profiles {
        let picked = s.ui.profile_pick.as_deref() == Some(p.name.as_str());
        // The store marks its own active profile (`active`); a stale marker would have been dropped by
        // the parser, so no row ever carries a mark it cannot justify.
        let loaded = session_name.as_deref() == Some(p.name.as_str());
        let (bullet, tone) = if loaded { ("◉ ", Gold) } else if p.active { ("● ", Mint) } else { ("○ ", Dim) };
        let pinned = r.pinned.as_deref() == Some(p.name.as_str());
        let mut left = if picked { vec![seg(bullet, tone), seg("▸ ", Gold), seg(p.name.clone(), Gold)] }
                   else { vec![seg(bullet, tone), seg(p.name.clone(), Text)] };
        // ▣ names the profile this repository's subagent launches resolve (the pin layer that won):
        // a square beside the store's own circular marks, so global-active (●) and repo-pinned never
        // read alike at 48 columns.
        if pinned { left.insert(1, seg("▣ ", Cyan)); }
        // A sendable name binds the profile to the current session (`/profile`, a session-only
        // binding); anything else keeps the old visual-only selection, so no stored string can
        // ever become a command argument.
        let action = if crate::core::is_valid_profile_name(&p.name) { Action::UseProfile(p.name.clone()) } else { Action::PickProfile(p.name.clone()) };
        // The store holds no descriptions: the role count and the host model are the distinctive data.
        let roles = if p.roles == 1 { "1 rol".to_string() } else { format!("{} roles", p.roles) };
        let mut right = vec![seg(roles, Dim)];
        if !p.host.is_empty() { right.push(seg(" · ", Text)); right.push(seg(p.host.clone(), Gold)); }
        // One row when it fits; at narrow widths the detail takes its own row (the `model_rows` pattern)
        // so the name is never clipped by its data.
        if width_of(&left) + width_of(&right) + 2 <= w.saturating_sub(2) {
            c.row(left, right);
            c.on_click(action.clone());
        } else {
            c.row(left, vec![]); c.on_click(action.clone());
            let mut sub = vec![seg("  ", Text)]; sub.extend(right);
            c.row(sub, vec![]); c.on_click(action);
        }
    }
    // What this repository's subagent launches resolve: the winning pin layer, or the global routing.
    if let Some(name) = &r.pinned {
        c.row(vec![seg("▣ ", Cyan), seg("subagentes: ", Text), seg(name.clone(), Cyan)], vec![seg("pineado", Dim)]);
    }
    // What the session itself resolves: the channel's binding (loaded by a click, or before the rail
    // started), or why no answer exists. The channel being down names the extension, not an error.
    if let Some(v) = session {
        c.row(vec![seg("◉ ", Gold), seg("sesión: ", Text), seg(text_of(session_profile(v), "name"), Gold)], vec![seg("cargado", Dim)]);
    } else if s.profile_channel_down {
        c.row(vec![seg("◉ ", Dim), seg("canal no disponible", Dim)], vec![seg("extensión", Dim)]);
    }
    Some(c)
}
/// `• name ··· model (effort)` on one row when it fits, else the model on its own indented row.
fn model_rows(c: &mut Card, left: Vec<Seg>, model: &str, effort: &str) {
    use Tone::*;
    let short = model.rsplit('/').next().unwrap_or_default(); let short = short.strip_suffix("-high").unwrap_or(short);
    let mut right = vec![seg(if short.is_empty() { "—" } else { short }, Gold)];
    if !effort.is_empty() { right.push(seg(format!(" ({effort})"), Dim)); }
    if width_of(&left) + width_of(&right) + 2 <= c.w.saturating_sub(4) { c.row(left, right); }
    else { c.row(left, vec![]); let mut sub = vec![seg("  ", Text)]; sub.extend(right); c.row(sub, vec![]); }
}
/// Subagent families of the reference (`getAgentStyle`): bullet and name tones.
fn agent_tones(name: &str) -> (Tone, Tone) {
    use Tone::*;
    if name.starts_with("gentle-ai-") { (Cyan, Cyan) } else if name.starts_with("jd-") { (Gold, Gold) }
    else if name.starts_with("review-") { (Mint, Mint) } else if name.starts_with("research-") { (Salmon, Salmon) } else { (Pink, Text) }
}
fn profiles_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let mut c = Card::new("profiles", mark("Profiles & Clusters", PinkBright), w);
    let r = s.roster.clone().unwrap_or_default();
    // A session profile is loaded through the channel: this card shows the loaded profile's
    // model map (host from its `orchestrator` entry, one row per role) instead of the global
    // store, with the session label naming what resolved it.
    let session = s.profile_channel.as_ref().and_then(|o| o.as_ref());
    if let Some(v) = session {
        let profile = session_profile(v);
        c.row(vec![seg("◉ ", Gold), seg("perfil de sesión: ", Text), seg(text_of(profile, "name"), Gold)], vec![]);
        // The extension reports what actually happened to the live model; the routing below is
        // the profile's configuration, so a failed switch must be visible, never implied.
        if profile["live"].as_str()==Some("failed") {
            c.row(vec![seg("  switch en vivo falló · sigue el modelo anterior", Coral)], vec![]);
        }
        let models = &profile["models"];
        let host = models["orchestrator"]["model"].as_str().unwrap_or("");
        let host_tier = models["orchestrator"]["tier"].as_str().unwrap_or("");
        // A profile without an `orchestrator` entry keeps the live host model (no live switch happened).
        let (model,thinking)=if host.is_empty() {
            match r.configured_host.as_ref() {Some(a)=>(a.model.as_str(),a.thinking.as_str()),None=>(r.model.as_str(),r.thinking.as_str())}
        } else {(host,host_tier)};
        model_rows(&mut c, vec![seg("🎯 ", Border), bold("host / orquestador", Border)], model, thinking);
        if let Some(entries) = models.as_object() {
            for (role,route) in entries {
                if role=="orchestrator" {continue}
                c.row(vec![], vec![]);
                model_rows(&mut c, vec![seg("• ", Salmon), seg(role.clone(), Salmon)], route["model"].as_str().unwrap_or(""), route["tier"].as_str().unwrap_or(""));
            }
        }
        c.divider();
    } else if s.profile_channel_down {
        c.row(vec![seg("canal de perfiles no disponible", Dim)], vec![seg("extensión", Dim)]);
        c.divider();
    } else if !r.profile_source.is_empty() {c.row(vec![seg(r.profile_source.clone(), Dim)], vec![]);}
    if session.is_none() {
    // A profile without an `orchestrator` entry keeps the host defaults of settings.json;
    // the store never rewrites STATUS, only this card.
    let (model,thinking)=match r.configured_host.as_ref() {
        Some(a)=>(a.model.as_str(),a.thinking.as_str()),
        None=>(r.model.as_str(),r.thinking.as_str()),
    };
    model_rows(&mut c, vec![seg("🎯 ", Border), bold("host / orquestador", Border)], model, thinking);
    if r.agents.is_empty() { c.row(vec![], vec![]); c.row(vec![seg("sin subagentes configurados", Dim)], vec![]); }
    for a in &r.agents {
        c.row(vec![], vec![]);
        let (bt, nt) = agent_tones(&a.name);
        // A role with no usable model says so: the rail never fills it with a guess.
        if a.model.is_empty() { c.row(vec![seg("• ", bt), seg(a.name.clone(), nt)], vec![seg("sin modelo", Coral)]); continue }
        model_rows(&mut c, vec![seg("• ", bt), seg(a.name.clone(), nt)], &a.model, &a.thinking);
    }
    }
    if r.total > 0 {
        c.divider();
        // Counted, not implied: `done/total` read as "the rest is pending" when the rest was failures.
        let title = vec![seg("📋 Tareas", Pink)];
        let mut right = vec![seg(format!("{} ✓", r.done), Mint)];
        if r.failed > 0 { right.extend([seg(" · ", Text), seg(format!("{} ✗", r.failed), Coral)]); }
        if r.active > 0 { right.extend([seg(" · ", Text), seg(format!("{} en curso", r.active), Gold)]); }
        // The title is short on purpose: "Task Manager" plus the counters did not fit in 42 columns and the
        // card clipped it to "Task Man". If even "Tareas" does not fit beside them, they get their own row.
        if width_of(&title) + width_of(&right) + 2 <= w.saturating_sub(2) { c.row(title, right) }
        else { c.row(title, vec![]); c.row(vec![], right); }
        // How long a task takes, on average: the number that says whether the machine is working or waiting.
        if !r.avg.is_empty() {
            c.row(vec![seg("   ⏱ ", Dim), seg(format!("{} promedio · {} medidas", r.avg, r.measured), Dim)], vec![]);
        }
        // Every task alive right now, newest first, with how long it has been going.
        for t in r.running.iter().take(5) {
            c.row(vec![seg("   ▶ ", Text), seg(t.text.clone(), Text), seg(format!(" · {}", t.elapsed), Dim)], vec![]);
        }
        if r.running.len() > 5 { c.row(vec![seg(format!("   … {} más en curso", r.running.len() - 5), Dim)], vec![]); }
    }
    // The tab's door to the screen itself: always present, with or without profiles, an explicit
    // click sends the fixed command to the bound pane (throttled by the pane worker).
    c.divider();
    c.row(vec![seg("▸ ", Gold), seg("/gentle:profiles", Gold)], vec![seg("abrir", Dim)]);
    c.on_click(Action::ProfilesCommand);
    c
}

// ---- 4:MEM ----------------------------------------------------------------------------
fn engram_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let e = &s.engram;
    let mut title = vec![seg("🧠 Engram", Pink)]; if !e.project.is_empty() { title.push(seg(": ", Pink)); title.push(seg(e.project.clone(), Cyan)); }
    let mut c = Card::new("engram", title, w);
    let (dot, tone) = if e.online { ("● Online", Mint) } else { ("○ Caído", Coral) };
    let obs = e.count.map_or("— obs".to_string(), |n| format!("{n} obs"));
    c.row(vec![seg("💻 ", Text), seg("Local (7437)", Text)], vec![seg(dot, tone), seg(" · ", Text), seg(obs, Cyan)]);
    if let Some(err) = &e.error { c.row(vec![seg(err.clone(), Coral)], vec![]); }
    c.divider();
    c.row(vec![seg("☁ ", Text), seg("Cloud", Text)], vec![]);
    match (&e.sync, e.enrolled) {
        (Some((phase, _)), true) if phase == "syncing" => c.row(vec![seg("   Sync:", Dim)], vec![seg("⟳ Sincronizando…", Yellow)]),
        (sync, true) => c.row(vec![seg("   Sync:", Dim)], vec![seg(format!("● Enrolado ({})", ago(sync.as_ref().and_then(|s| s.1), now_secs())), Mint)]),
        _ => c.row(vec![seg("   Estado:", Dim)], vec![seg("○ No sincronizado", Dim)]),
    }
    c
}
/// Handoff prose highlights of the reference: base branches mint, `#N` gold, arrows as a pink `➔`.
fn highlight(text: &str, base: Tone) -> Vec<Seg> {
    let mut out = Vec::new();
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 { out.push(seg(" ", base)); }
        let bare = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '#');
        let tone = match bare { "main" | "develop" | "master" => Tone::Mint, _ if bare.len() > 1 && bare.starts_with('#') && bare[1..].chars().all(|c| c.is_ascii_digit()) => Tone::Gold, _ => base };
        match word { "->" | "➔" => out.push(seg("➔", Tone::Pink)), _ => out.push(seg(word, tone)) }
    }
    out
}
const HANDOFF_ROWS: usize = 24;
fn handoff_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let mut c = Card::new("handoff", vec![seg("🧠 Active Handoff", Gold)], w);
    let Some(h) = &s.engram.handoff else {
        c.row(vec![seg("Sin resumen de sesión previo", Dim)], vec![]); c.row(vec![seg("(se registra con mem_session_summary)", Dim)], vec![]); return c;
    };
    c.row(vec![seg(format!("#{}", h.id), Cyan), seg(" ", Text), seg(h.kind.clone(), Gold)], vec![seg(ago(h.at, now_secs()), Muted)]);
    c.divider(); c.row(vec![], vec![]);
    let inner = w.saturating_sub(4); let mut body: Vec<Vec<Seg>> = Vec::new();
    for l in &h.lines {
        if let Some(item) = ["- ", "* ", "• "].iter().find_map(|p| l.strip_prefix(p)) {
            let mut r = vec![seg("  • ", Pink)]; r.extend(fit(highlight(item.trim(), Cyan), inner.saturating_sub(5))); body.push(r);
        } else if l.ends_with(':') || l.starts_with('#') {
            body.push(vec![seg("✦ ", Pink), seg(l.trim_start_matches('#').trim(), Gold)]);
        } else { for p in wrap(l, inner.saturating_sub(2)) { body.push(highlight(&p, Cyan)); } }
    }
    let more = body.len().saturating_sub(HANDOFF_ROWS);
    for r in body.into_iter().take(HANDOFF_ROWS) { c.row(r, vec![]); }
    if more > 0 { c.row(vec![seg(format!("… {more} líneas más"), Dim)], vec![]); }
    c.row(vec![], vec![]);
    c
}
fn stack_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let mut c = Card::new("stack", [mark("Stack", Gold), vec![seg(" · ", Muted), seg("Ecosistema", Pink)]].concat(), w);
    if s.stack.is_empty() { c.row(vec![seg("Detectando herramientas…", Dim)], vec![]); }
    for t in &s.stack {
        let accent = match t.name { "pi" | "herdr" => Cyan, "gs3-hud" => Pink, "gentle-shell" => Gold, "gentle-ai" => Mint, "engram" => Secondary, _ => Text };
        match &t.version {
            Some(v) => c.row(vec![seg("● ", Mint), seg(t.name, accent), seg(format!(" v{v}"), Text)], vec![]),
            None => c.row(vec![seg("○ ", Dim), seg(t.name, Dim), seg(" · no verificada", Dim)], vec![]),
        }
    }
    c
}
/// Reference `formatToolPill` order and tones; zero counts are skipped.
fn tool_pills(t: &Session) -> Vec<Vec<Seg>> {
    use Tone::*;
    [(t.reads, "✎", "read", Read), (t.writes, "✎", "write", Cyan), (t.bash, ">_", "bash", Green), (t.mem, "🧠", "engram", Salmon),
     (t.grep, "🔍", "grep", Yellow), (t.fetch, "↓", "fetch", Pink), (t.search, "◆", "search", Secondary), (t.other, "⚙", "other", Dim)]
        .into_iter().filter(|p| p.0 > 0).map(|(n, g, l, tone)| vec![seg(format!("{g} {n} {l}"), tone)]).collect()
}
fn tools_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let t = &s.session; let mut c = Card::new("tools", vec![seg("🔧 tools", Gold)], w);
    c.extra = vec![(seg(format!("{} calls", t.tools), Dim), None)];
    let pills = tool_pills(t);
    if pills.is_empty() { c.row(vec![seg("sin llamadas todavía", Dim)], vec![]); }
    // `wrapToolPills`: 3 spaces between pills, a new row when the next one does not fit.
    let inner = w.saturating_sub(4); let mut line: Vec<Seg> = Vec::new();
    for p in pills {
        if !line.is_empty() && width_of(&line) + 3 + width_of(&p) > inner { c.row(std::mem::take(&mut line), vec![]); }
        if !line.is_empty() { line.push(seg("   ", Text)); }
        line.extend(p);
    }
    if !line.is_empty() { c.row(line, vec![]); }
    c
}
fn mcp_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let mut title = mark("MCP", PinkBright);
    if let Some(l) = &s.mcp { title.push(seg(format!(" · {}", l.len()), Muted)); }
    let mut c = Card::new("mcp", title, w);
    match &s.mcp {
        Some(list) => for m in list { c.row(vec![seg("● ", Mint), seg(m.name.clone(), Text)], vec![seg(if m.local { "[local]" } else { "[remote]" }, Dim)]); },
        None => c.row(vec![seg("sin mcp.json", Dim)], vec![]),
    }
    c
}

// ---- 6:MEDIA --------------------------------------------------------------------------
/// Upper section of MEDIA: Spotify. Without a credential file the card states the prerequisite and
/// the exact redirect the app must allowlist; with one it lists the user's own playlists and offers
/// a renewal instead of first-time setup.
fn spotify_card(s: &State, w: usize, show_cover: bool) -> Card {
    use Tone::*;
    let sp = &s.media.spotify;
    let mut c = Card::new("spotify", mark("Spotify", Mint), w);
    if show_cover {
        for _ in 0..6 { c.row(vec![], vec![]); }
    }
    c.row(vec![seg("estado · ", Muted), seg(&sp.state, Text)], vec![]);
    if !sp.linked {
        c.row(vec![seg("creá la app en developer.spotify.com", Dim)], vec![]);
        c.row(vec![seg("guardá el client_id en", Dim)], vec![]);
        c.row(vec![seg("~/.config/gs3/spotify.json", Dim)], vec![]);
        c.row(vec![seg("redirect exacto", Dim)], vec![]);
        c.row(vec![seg(REDIRECT, Dim)], vec![]);
        let row = c.rows.len();
        c.row(vec![seg("↗ conectar", Pink)], vec![seg("abre el navegador", Dim)]);
        c.hits.push((row, 2, 12, Action::SpotifyAuthorize));
        return c;
    }
    let ready = sp.running && sp.device_id.is_some();
    let row = c.rows.len();
    let mut left = vec![seg(if sp.running { "↻ dispositivo" } else { "▶ dispositivo" }, Pink)];
    let device_end = width_of(&left);
    let search_start = if s.ui.spotify_prompt.is_none() {
        left.push(seg(" · ", Muted));
        let start = width_of(&left);
        left.push(seg("🔍 buscar", if ready { Pink } else { Dim }));
        Some(start)
    } else { None };
    let search_end = width_of(&left);
    let hint = if !ready && search_start.is_some() { vec![seg("sin dispositivo", Muted)] } else { vec![] };
    let available = w.saturating_sub(4).saturating_sub(if hint.is_empty() { 0 } else { width_of(&fit(hint.clone(), w.saturating_sub(4))) + 1 });
    c.row(left, hint);
    if device_end.min(available) > 0 { c.hits.push((row, 2, 2 + device_end.min(available), Action::SpotifyStart)); }
    if let Some(start) = search_start { if ready && start < available { c.hits.push((row, 2 + start, 2 + search_end.min(available), Action::SpotifySearch)); } }
    if let Some(url) = &sp.oauth_url {
        let width = w.saturating_sub(4).clamp(1, 30);
        let chars: Vec<char> = url.chars().collect();
        for chunk in chars.chunks(width) { c.row(vec![seg(chunk.iter().collect::<String>(), Text)], vec![]); }
    }
    if let Some(buf) = &s.ui.spotify_prompt {
        // The hint owns its own row: sharing it with the input cut long queries at half the inner width.
        c.row(vec![seg("> ", PinkBright), seg(buf.clone(), Text), bold("▏", PinkBright)], vec![]);
        c.row(vec![seg("Enter busca · Esc cancela", Dim)], vec![]);
    } else {
        if sp.search_done && sp.results.is_empty() { c.row(vec![seg("sin resultados", Dim)], vec![]); }
        for (index, track) in sp.results.iter().enumerate() {
            let row = c.rows.len();
            c.row(vec![seg(format!("{:>2}. ", index + 1), Dim), seg(&track.title, Text)], vec![seg(&track.artist, Muted)]);
            if ready { c.hits.push((row, 2, w.saturating_sub(2), Action::SpotifyPlayResult(index))); }
        }
    }
    let playable = ready && sp.selected.and_then(|i| sp.playlists.get(i)).is_some() && !sp.tracks.is_empty();
    let row = c.rows.len();
    let pausing = ready && sp.playing;
    c.row(vec![seg(if pausing { "❚❚ pausar" } else { "▶ reproducir lista" }, if pausing || playable { Pink } else { Dim })], vec![]);
    if pausing || playable { c.hits.push((row, 2, w.saturating_sub(2), if pausing { Action::SpotifyPause } else { Action::SpotifyPlay(0) })); }
    let row = c.rows.len();
    let loudness = sp.volume.map(|v| format!("{v}%")).unwrap_or_else(|| "—".into());
    let mut left = vec![seg("vol ", Muted)];
    let volume_start = width_of(&left);
    left.extend(volume_bar(sp.volume));
    left.push(seg(format!(" {loudness}   "), Muted));
    let shuffle_start = width_of(&left);
    left.push(seg(format!("⇄ {}", if sp.shuffle { "sí" } else { "no" }), if ready { Pink } else { Dim }));
    let shuffle_end = width_of(&left);
    left.push(seg("   ", Muted));
    let repeat_start = width_of(&left);
    left.push(seg(format!("⟲ {}", sp.repeat.label()), if ready { Pink } else { Dim }));
    let repeat_end = width_of(&left);
    c.row(left, vec![]);
    let inner = w.saturating_sub(4);
    if ready {
        for cell in 0..VOLUME_CELLS { if volume_start + cell < inner { c.hits.push((row, 2 + volume_start + cell, 3 + volume_start + cell, Action::SpotifyVolume(volume_percent(cell)))); } }
        if shuffle_start < inner { c.hits.push((row, 2 + shuffle_start, 2 + shuffle_end.min(inner), Action::SpotifyShuffle(!sp.shuffle))); }
        if repeat_start < inner { c.hits.push((row, 2 + repeat_start, 2 + repeat_end.min(inner), Action::SpotifyRepeat(sp.repeat.cycle()))); }
    }
    if let Some(title) = &sp.title {
        c.row(vec![seg(title, Text)], vec![seg(&sp.artist, Muted)]);
        let row = c.rows.len();
        c.row(vec![seg("◀◀   ", Muted), seg(if sp.playing { "❚❚" } else { "▶ " }, PinkBright), seg("   ▶▶", Muted)], vec![]);
        if ready {
            c.hits.push((row, 2, 4, Action::SpotifyPrevious));
            c.hits.push((row, 7, 9, if sp.playing { Action::SpotifyPause } else { Action::SpotifyResume }));
            c.hits.push((row, 12, 14, Action::SpotifyNext));
        }
        let inner = w.saturating_sub(4);
        let fraction = if sp.duration_ms > 0 { (sp.position_ms as f64 / sp.duration_ms as f64).clamp(0.0, 1.0) } else { 0.0 };
        let row = c.rows.len(); c.row(seek_bar(fraction, inner), vec![]);
        if ready && sp.duration_ms > 0 && inner > 0 {
            let last = inner.saturating_sub(1).max(1);
            for k in 0..inner { c.hits.push((row, 2 + k, 3 + k, Action::SpotifySeek(k as f64 / last as f64))); }
        }
        c.row(vec![seg(format!("{} / {}", clock(sp.position_ms as f64 / 1000.0), clock(sp.duration_ms as f64 / 1000.0)), Muted)], vec![seg(&sp.state, Dim)]);
    }
    for (index, playlist) in sp.playlists.iter().enumerate() {
        let picked = sp.selected == Some(index);
        let row = c.rows.len();
        c.row(
            vec![
                seg(if picked { "● " } else { "○ " }, if picked { Mint } else { Dim }),
                seg(&playlist.name, if picked { PinkBright } else { Text }),
            ],
            vec![],
        );
        c.hits.push((row, 2, w.saturating_sub(2), Action::SelectPlaylist(playlist.id.clone())));
    }
    if sp.playlists.is_empty() {
        c.row(vec![seg("sin listas propias todavía", Dim)], vec![]);
    }
    for (label, action, end) in [("↻ traer mis listas", Action::SpotifyPlaylists, 19), ("↻ renovar sesión", Action::SpotifyRefresh, 18)] {
        let row = c.rows.len();
        c.row(vec![seg(label, Pink)], vec![seg(if end == 19 { "solo lectura" } else { "por si expiró" }, Dim)]);
        c.hits.push((row, 2, end, action));
    }
    c
}

/// The rows the `⌁ Pistas` card shows at once. The card is a window, not a copy of the playlist: a
/// fifty-track list used to be cut at ten, and showing all fifty would push the YouTube section off
/// the rail entirely. The wheel over the card scrolls the rest.
const TRACK_ROWS: usize = 10;

/// The tracks of the selected playlist, as a window of [`TRACK_ROWS`] rows: `S13` lets the whole list
/// be reached by scrolling inside the card, while `S2` keeps the list and its music side by side.
fn tracks_card(s: &State, w: usize, scroll: usize) -> Option<Card> {
    use Tone::*;
    let tracks = &s.media.spotify.tracks;
    if tracks.is_empty() { return None }
    let max_scroll = tracks.len().saturating_sub(TRACK_ROWS);
    let first = scroll.min(max_scroll);
    let last = (first + TRACK_ROWS).min(tracks.len());
    let mut c = Card::new("pistas", mark("Pistas", Pink), w);
    // Which rows are on screen: without it a windowed list looks like the whole playlist.
    if max_scroll > 0 { c.extra = vec![(seg(format!("{}-{}/{} ", first + 1, last, tracks.len()), Dim), None)]; }
    for (index, track) in tracks.iter().enumerate().take(last).skip(first) {
        // S12: the row `/me/player` names by id is the one being heard. A track started from the search
        // box is not in this list, so it marks nothing here.
        let playing = s.media.spotify.playing_id.as_deref() == Some(track.id.as_str());
        let row = c.rows.len();
        let number = if playing { vec![seg("▶ ", PinkBright), seg(format!("{:>2}. ", index + 1), Mint), seg(&track.title, PinkBright)] } else { vec![seg(format!("{:>2}. ", index + 1), Dim), seg(&track.title, Text)] };
        c.row(number, vec![seg(&track.artist, Muted)]);
        if s.media.spotify.running && s.media.spotify.device_id.is_some() { c.hits.push((row, 2, w.saturating_sub(2), Action::SpotifyPlay(index))); }
    }
    c.scrolls(first, max_scroll);
    Some(c)
}

/// Lower section of MEDIA: YouTube. mpv draws the video in its own window; this card is the control.
fn youtube_card(s: &State, w: usize) -> Card {
    use Tone::*;
    let y = &s.media.youtube;
    let inner = w.saturating_sub(4);
    let mut c = Card::new("youtube", mark("YouTube", Coral), w);
    // S3/S4: the dialog that searches a specific track. Modal, so the card shows only the box.
    if let Some(buf) = &s.ui.prompt {
        // The hint owns its own row: sharing it with the input cut long queries at half the inner width.
        c.row(vec![seg("> ", PinkBright), seg(buf.clone(), Text), bold("▏", PinkBright)], vec![]);
        c.row(vec![seg("Enter busca · Esc cancela", Dim)], vec![]);
        c.video_rows();
        return c;
    }
    let search = "🔍 buscar"; let sep = " · "; let lists = "🔍 listas";
    let hint = "el vídeo se abre en mpv";
    let hint = if search.width() + sep.width() + lists.width() + 1 + hint.width() <= inner { vec![seg(hint, Dim)] } else { vec![] };
    c.row(vec![seg(search, Pink), seg(sep, Dim), seg(lists, Pink)], hint);
    let start = search.width() + sep.width();
    if start < inner { c.hits.push((0, 2 + start, 2 + (start + lists.width()).min(inner), Action::YtLists)); }
    if let Some(buf) = &s.ui.yt_lists {
        c.row(vec![seg("> ", PinkBright), seg(buf.clone(), Text), bold("▏", PinkBright)], vec![]);
        c.row(vec![seg("Enter busca · Esc cancela", Dim)], vec![]);
    }
    for playlist in y.playlists.iter().filter(|_| y.playlist_view) {
        c.row(vec![seg(&playlist.title, Text)], vec![seg("lista", Muted)]);
        c.on_click(Action::YtOpenPlaylist(playlist.id.clone()));
    }
    // Only a video that exists or is about to be started reserves rows; an idle card spends nothing.
    if y.title.is_some() {
        c.row(vec![], vec![]);
        c.video_rows();
    }
    if y.title.is_some() && !y.queue.is_empty() {
        c.row(vec![seg(y.title.clone().unwrap_or_default(), Text)], vec![seg(format!("{}/{}", y.index + 1, y.queue.len()), Dim)]);
        c.row(vec![seg("◀◀   ", Muted), seg(if y.playing { "❚❚" } else { "▶ " }, PinkBright), seg("   ▶▶", Muted)], vec![]);
        let row = c.rows.len() - 1;
        c.hits.push((row, 2, 4, Action::PrevTrack));
        c.hits.push((row, 7, 9, Action::PlayPause));
        c.hits.push((row, 12, 14, Action::NextTrack));
        let fraction = if y.duration > 0.0 { (y.position / y.duration).clamp(0.0, 1.0) } else { 0.0 };
        c.row(seek_bar(fraction, inner), vec![]);
        // S5: every column of the bar seeks to its own position.
        if inner > 0 {
            let row = c.rows.len() - 1; let last = inner.saturating_sub(1).max(1);
            for k in 0..inner { c.hits.push((row, 2 + k, 3 + k, Action::Seek(k as f64 / last as f64))); }
        }
        if y.state.starts_with("sin geometría:") {
            c.row(vec![seg(format!("{} / {}", clock(y.position), clock(y.duration)), Muted)], vec![]);
            for line in wrap(&y.state, inner) { c.row(vec![seg(line, Dim)], vec![]); }
        } else {
            // A paused player used to keep saying `reproduciendo`: the word came from the last command,
            // not from what mpv reports, and it read as if pause had failed.
            let state = if !y.playing && y.state == "reproduciendo" { "en pausa" } else { y.state.as_str() };
            c.row(vec![seg(format!("{} / {}", clock(y.position), clock(y.duration)), Muted)], vec![seg(state, Dim)]);
        }
    } else {
        c.row(vec![seg("estado · ", Muted), seg(&y.state, Text)], vec![]);
    }
    // The whole card opens the search box, not only its last row: while a track plays the last row is
    // the progress line and the search row used to stop answering.
    for row in 0..c.rows.len() { c.hits.push((row, 2, w.saturating_sub(2), Action::Search)); }
    c
}

fn videos_card(s: &State, w: usize) -> Option<Card> {
    use Tone::*;
    let y = &s.media.youtube; if y.videos.is_empty() || s.ui.prompt.is_some() { return None }
    let max = y.videos.len().saturating_sub(TRACK_ROWS); let first = s.ui.video_scroll.min(max); let last = (first + TRACK_ROWS).min(y.videos.len());
    let mut c = Card::new("videos", mark(if y.from_playlist { "Videos" } else { "Sugeridos" }, Pink), w);
    if max > 0 { c.extra = vec![(seg(format!("{}-{}/{} ", first + 1, last, y.videos.len()), Dim), None)]; }
    // Both sources end up in `queue`, so the row being heard is the one the queue points at.
    let playing = y.queue.get(y.index).map(|v| v.id.as_str());
    for (i, v) in y.videos.iter().enumerate().take(last).skip(first) {
        let active = playing == Some(v.id.as_str());
        let mut left = Vec::new(); if active { left.push(seg("▶ ", PinkBright)); }
        left.push(seg(format!("{:>2}. ", i + 1), Dim)); left.push(seg(&v.title, if active { PinkBright } else { Text }));
        c.row(left, vec![]); c.on_click(Action::YtPlayVideo(i));
    }
    c.scrolls(first, max); Some(c)
}

/// `m:ss`, or `h:mm:ss` past an hour. Negative and non-finite positions are drawn as zero.
fn clock(seconds: f64) -> String {
    let total = if seconds.is_finite() && seconds > 0.0 { seconds as u64 } else { 0 };
    if total >= 3600 { format!("{}:{:02}:{:02}", total / 3600, total % 3600 / 60, total % 60) } else { format!("{}:{:02}", total / 60, total % 60) }
}

/// The volume bar: ten cells, each one a percentage the account can take.
const VOLUME_CELLS: usize = 10;
/// The percentage a click on `cell` sets; the last cell is 100 and the first is silence.
fn volume_percent(cell: usize) -> u64 { (cell.min(VOLUME_CELLS - 1) * 100 / (VOLUME_CELLS - 1)) as u64 }
/// Filled cells for a percentage, rounded up so a quiet setting is never drawn as silence.
fn volume_bar(volume: Option<u64>) -> Vec<Seg> {
    use Tone::*;
    let filled = volume.map(|v| ((v.min(100) as usize + 9) / 10).min(VOLUME_CELLS)).unwrap_or(0);
    vec![seg("▰".repeat(filled), Mint), seg("▱".repeat(VOLUME_CELLS - filled), Dim)]
}

/// The seek bar itself: played `━`, the current position `●`, and the rest `┄`.
fn seek_bar(fraction: f64, inner: usize) -> Vec<Seg> {
    use Tone::*;
    if inner == 0 { return Vec::new() }
    let at = (fraction.clamp(0.0, 1.0) * inner.saturating_sub(1) as f64).round() as usize;
    vec![seg("━".repeat(at), Mint), seg("●", PinkBright), seg("┄".repeat(inner.saturating_sub(at + 1)), BorderMuted)]
}

// ---- 7:CONFIG -------------------------------------------------------------------------
fn config_cards(s: &State, w: usize) -> Vec<Card> {
    use Tone::*;
    let ui = &s.ui;
    let mut name = Card::new("config-name", mark("Nombre", PinkBright), w);
    match &ui.editing {
        Some(buf) => {
            name.row(vec![seg("> ", Pink), seg(buf.clone(), Text), bold("▏", PinkBright)], vec![seg(format!("{}/{NAME_MAX}", buf.chars().count()), Dim)]);
            name.row(vec![seg("Enter guarda · Esc cancela", Dim)], vec![]);
        }
        None => {
            name.keyed("Nombre", vec![if ui.config.name.is_empty() { seg(" - - ", Dim) } else { seg(ui.config.name.clone(), PinkBright) }]);
            name.row(vec![seg("✎ editar", Pink)], vec![seg("vacío = \" - - \"", Dim)]); name.on_click(Action::Edit);
        }
    }
    let mut tabs = Card::new("config-tabs", mark("Pestañas", PinkBright), w);
    for t in Tab::ALL {
        let on = ui.config.visible(t);
        let right = if t == Tab::Config { seg("fija", Dim) } else if on { seg("visible", Mint) } else { seg("oculta", Dim) };
        let mut l = vec![seg(if on { "[✓] " } else { "[ ] " }, if on { Mint } else { Dim })];
        if t == Tab::Config { l.extend([seg(CONFIG_ICON, Purple), seg(" CONFIG", Text)]); } else { l.push(seg(t.label(), if on { Text } else { Muted })); }
        tabs.row(l, vec![right]);
        if t != Tab::Config { tabs.on_click(Action::Hide(t)); }
    }
    let mut mon = Card::new("config-monitor", mark("Monitor", PinkBright), w);
    mon.keyed("Panel", vec![seg(s.pane.clone(), Text)]);
    let file = match &ui.config_path { Some(p) => seg(display_path(p), Text), None => seg("sin --config (no se guarda)", Dim) };
    mon.keyed("Archivo", vec![file]);
    if let Some(e) = &ui.save_error { mon.row(vec![seg(format!("error al guardar: {e}"), Coral)], vec![]); }
    mon.divider();
    mon.row(vec![seg("1–7 ", Gold), seg("pestañas · ", Muted), seg("r ", Gold), seg("refrescar · ", Muted), seg("q ", Gold), seg("salir", Muted)], vec![]);
    mon.row(vec![seg("[-] ", Gold), seg("minimiza/maximiza · rueda: scroll", Muted)], vec![]);
    vec![name, tabs, mon]
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr as W;
    fn find<'a>(lines: &'a [String], needle: &str) -> &'a str { lines.iter().find(|l| l.contains(needle)).unwrap_or_else(|| panic!("no line with {needle:?} in {lines:#?}")) }
    fn on(s: &State, t: Tab, w: u16) -> Vec<String> { let mut s = s.clone(); s.ui.tab = t; render_lines(&s, w) }
    fn at(f: &Frame, needle: &str) -> (usize, usize) {
        let lines: Vec<String> = f.lines.iter().map(|l| plain(l)).collect();
        let r = lines.iter().position(|l| l.contains(needle)).unwrap_or_else(|| panic!("no {needle:?} in {lines:#?}"));
        (lines[r][..lines[r].find(needle).unwrap()].width(), r)
    }
    fn sample() -> State {
        let mut s = State::new("w1:p1".into(), "/srv/repo".into()); s.status = "working".into(); s.connected = true;
        s.session = Session { input: 136, output: 14_400, read: 33_536, write: 0, used: 33_677, cost: 0.25, model: "cpamc/gpt-6.1-sol".into(), provider: "cpamc".into(), thinking: "medium".into(), limit: Some(370_000), tools: 5, reads: 1, writes: 2, mem: 1, bash: 1, ..Session::default() };
        s.git = Git { branch: "feat/x".into(), dirty: Some(2), ahead_behind: Some((1, 0)), commits: vec!["abc1234 first commit".into()], ..Git::default() };
        s.engram = Engram { project: "repo".into(), count: Some(7), online: true, ..Engram::default() };
        s.mcp = Some(vec![Mcp { name: "engram".into(), local: true }]);
        s
    }
    #[test]
    fn both_search_boxes_put_the_hint_below_so_the_query_keeps_the_width() {
        let query = "around the world daft punk";
        let mut s = sample();
        s.ui.tab = Tab::Media;
        // Spotify's box: the hint used to sit on the input row and eat its width.
        s.media.spotify.linked = true;
        s.media.spotify.running = true;
        s.media.spotify.device_id = Some("d".into());
        s.ui.spotify_prompt = Some(query.into());
        let f = frame(&s, 42, 40);
        let lines: Vec<String> = f.lines.iter().map(|l| plain(l)).collect();
        let input = lines.iter().position(|l| l.contains("> around")).expect("spotify input row");
        assert!(lines[input].contains(query), "the hint must not take the query's width: {:?}", lines[input]);
        assert!(!lines[input].contains("Enter busca"), "the hint is not on the input row: {:?}", lines[input]);
        assert!(lines[input + 1].contains("Enter busca · Esc cancela"), "{:?}", lines[input + 1]);
        // YouTube's box had the same defect and moves with it.
        s.ui.spotify_prompt = None;
        s.ui.prompt = Some(query.into());
        let f = frame(&s, 42, 40);
        let lines: Vec<String> = f.lines.iter().map(|l| plain(l)).collect();
        let input = lines.iter().position(|l| l.contains("> around")).expect("youtube input row");
        assert!(lines[input].contains(query), "{:?}", lines[input]);
        assert!(!lines[input].contains("Enter busca"), "{:?}", lines[input]);
        assert!(lines[input + 1].contains("Enter busca · Esc cancela"), "{:?}", lines[input + 1]);
    }

    #[test]
    fn youtube_video_stays_in_reserved_rows_as_spotify_changes_height() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        s.media.youtube.title = Some("Unique video title".into());
        s.media.youtube.queue = vec![crate::media::Video { id: "a".into(), title: "Unique video title".into() }];
        s.media.youtube.state = "unique state".into();
        let check = |f: &Frame| {
            let lines: Vec<_> = f.lines.iter().map(|l| plain(l)).collect();
            let search = lines.iter().position(|l| l.contains("🔍 buscar")).unwrap();
            let title = lines.iter().position(|l| l.contains("Unique video title")).unwrap();
            let transport = lines.iter().position(|l| l.contains("◀◀")).unwrap();
            let state = lines.iter().position(|l| l.contains("unique state")).unwrap();
            let r = f.youtube_rect.unwrap();
            assert!(r.y as usize > search);
            assert!((r.y + r.height) as usize <= title, "video last row {} overlaps title row {}", r.y + r.height - 1, title);
            assert!((r.y + r.height) as usize <= transport && (r.y + r.height) as usize <= state);
            (search, r.y)
        };
        let a = check(&frame(&s, 42, 0));
        s.media.spotify.cover_url = Some("https://example.test/image".into());
        s.media.spotify.cover_png = Some(vec![1]);
        let b = check(&frame(&s, 42, 0));
        assert_eq!(b.0 - a.0, (b.1 - a.1) as usize);
        assert_eq!(b.0 - a.0, 6);
        // The picture owns the block from its first reserved row.
        assert_eq!((a.1, b.1), (20, 26));
        s.ui.config.collapsed.push("spotify".into());
        let collapsed = check(&frame(&s, 42, 0));
        assert!(collapsed.0 < b.0);
        assert_eq!(b.0 - collapsed.0, (b.1 - collapsed.1) as usize);
        s.media.spotify.tracks.push(crate::media::Track { id: "t".into(), title: "Track".into(), artist: "Artist".into(), position: 0 });
        let tracks = check(&frame(&s, 42, 0));
        assert!(tracks.0 > collapsed.0);
        assert_eq!(tracks.0 - collapsed.0, (tracks.1 - collapsed.1) as usize);
    }

    #[test]
    fn youtube_prompt_block_and_offscreen_visibility() {
        let mut s = sample(); s.ui.tab = Tab::Media; s.ui.prompt = Some("query".into());
        let f = frame(&s, 42, 0);
        let lines: Vec<_> = f.lines.iter().map(|l| plain(l)).collect();
        let hint = lines.iter().position(|l| l.contains("Enter busca · Esc cancela")).unwrap();
        let end = (hint + 1..lines.len()).find(|&i| lines[i].contains('╚')).unwrap();
        let r = f.youtube_rect.unwrap();
        assert!(r.y as usize > hint && (r.y + r.height) as usize <= end);
        s.ui.tab = Tab::Main;
        assert_eq!(frame(&s, 42, 0).youtube_rect, None);
        s.ui.tab = Tab::Media;
        s.ui.config.collapsed.push("youtube".into());
        assert_eq!(frame(&s, 42, 0).youtube_rect, None);
        s.ui.config.collapsed.clear();
        assert_eq!(frame(&s, 42, 10).youtube_rect, None);
        s.ui.scroll[Tab::Media.index()] = usize::MAX;
        assert_eq!(frame(&s, 42, 10).youtube_rect, None);
    }

    #[test]
    fn youtube_placement_and_fallback_remain_visible() {
        let mut s = sample(); s.ui.tab = Tab::Media; s.ui.prompt = Some("query".into());
        let prompt = frame(&s, 42, 0);
        let rect = prompt.youtube_rect.expect("video area");
        let input = prompt.lines.iter().position(|l| plain(l).contains("> query")).unwrap();
        assert_eq!(rect.y as usize, input + 2 + (VIDEO_ROWS - VIDEO_ROWS_TALL) / 2, "the video is centred in the reserved rows");
        s.ui.prompt = None;
        s.media.youtube.state = "sin geometría: sin ubicación de ventana del terminal".into();
        s.media.youtube.title = Some("video".into());
        s.media.youtube.queue = vec![crate::media::Video { id: "a".into(), title: "video".into() }];
        let f = frame(&s, 42, 0);
        assert_eq!(f.youtube_rect, Some(rect));
        assert!(f.lines.iter().any(|l| plain(l).contains("sin geometría")));
        assert!(f.lines.iter().any(|l| plain(l).contains("sin geometría")));
    }

    #[test]
    fn the_whole_youtube_card_opens_the_search_box_while_a_video_plays() {
        // The search row used to stop working the moment a video started, because the card registered
        // the action on its last row only and the last row is then the progress line.
        let mut s = sample(); s.ui.tab = Tab::Media;
        s.media.youtube.title = Some("The Weeknd".into());
        s.media.youtube.queue = vec![crate::media::Video { id: "a".into(), title: "The Weeknd".into() }];
        s.media.youtube.duration = 247.0;
        let f = frame(&s, 42, 60);
        let (x, y) = at(&f, "buscar");
        assert_eq!(click(&f, x, y), Some(Action::Search), "the search row works while playing");
        // The transport keeps its own, narrower hitboxes.
        let (px, py) = at(&f, "▶");
        assert_eq!(click(&f, px, py), Some(Action::PlayPause));
    }

    #[test]
    fn the_video_block_is_reserved_only_while_it_can_show_a_video() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        let idle = frame(&s, 42, 40);
        assert_eq!(idle.youtube_rect, None, "an idle card must not spend six empty rows");
        s.ui.prompt = Some("daft punk".into());
        let open = frame(&s, 42, 40);
        assert!(open.youtube_rect.is_some(), "the box reserves the block for the video it is about to start");
        s.ui.prompt = None;
        s.media.youtube.title = Some("One More Time".into());
        let playing = frame(&s, 42, 40);
        assert!(playing.youtube_rect.is_some(), "a playing video owns the block");
        // Twelve rows is the closest a card this narrow gets to 16:9, so the video has almost no
        // letterbox left to crop and the window stays inside the frame.
        assert_eq!(playing.youtube_rect.unwrap().height, VIDEO_ROWS_TALL as u32, "the video rect is the window's size");
        // The picture fills the card's inner span edge to edge: one cell in from the left border,
        // ending where the right border starts. Any inset shows up as an empty band beside the video.
        let r = playing.youtube_rect.unwrap();
        let width = 42usize;
        let inner = width - 2 * PAD - 2;
        assert_eq!(r.x as usize, PAD + 1 + VIDEO_SIDE, "left edge one cell in from the inner border");
        assert_eq!(r.width as usize, inner - VIDEO_SIDE * 2, "and the same cell of air on the right");
        assert_eq!((r.x + r.width) as usize, width - 1 - PAD - VIDEO_SIDE, "never touching the right border");
        assert_eq!(r.height as usize, VIDEO_ROWS_TALL);
        assert!(playing.lines.len() > idle.lines.len(), "the block is real rows, not an invisible promise");
    }

    #[test]
    fn cover_layout_is_reserved_only_for_visible_complete_images() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        let empty = frame(&s, 42, 0);
        assert_eq!(empty.cover_pos, None);
        s.media.spotify.cover_url = Some("https://example.test/image".into());
        s.media.spotify.cover_png = Some(vec![1]);
        let shown = frame(&s, 42, 0);
        assert_eq!(shown.cover_pos, Some((3, 8)));
        assert_eq!(shown.lines.len(), empty.lines.len() + 6);
        s.ui.scroll[Tab::Media.index()] = 9;
        assert_eq!(frame(&s, 42, 20).cover_pos, None);
        s.ui.tab = Tab::Main;
        assert_eq!(frame(&s, 42, 30).cover_pos, None);
    }

    #[test]
    fn scrolling_the_spotify_card_leaves_no_cover_sized_hole() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        s.media.spotify.cover_url = Some("https://example.test/image".into());
        s.media.spotify.cover_png = Some(vec![1]);
        s.ui.scroll[Tab::Media.index()] = 9;
        let scrolled = frame(&s, 42, 20);
        assert_eq!(scrolled.cover_pos, None);
        // The six reserved rows must go with the picture: the body has to be exactly the one a card
        // without a cover has, or the reader gets a blank block that promises an image it never shows.
        s.media.spotify.cover_png = None;
        assert_eq!(scrolled.max_scroll, frame(&s, 42, 20).max_scroll);
    }
    #[test]
    fn spotify_compact_controls_have_independent_hits() {
        let mut s = sample(); long_playlist(&mut s, 1);
        s.media.spotify.volume = None;
        let c = spotify_card(&s, 46, false);
        let lines: Vec<String> = c.rows.iter().map(|r| plain(r)).collect();
        assert!(lines[1].contains("↻ dispositivo · 🔍 buscar"), "{:?}", lines);
        assert!(lines[2].contains("▶ reproducir lista"));
        assert!(lines[3].contains("vol ▱▱▱▱▱▱▱▱▱▱ —   ⇄ no   ⟲ no"));
        assert!(lines.iter().all(|l| !l.contains("aleatorio") && !l.contains("repetir ·") && !l.contains("pausar")));
        let f = frame(&s, 48, 0);
        for (label, action) in [("↻ dispositivo", Action::SpotifyStart), ("🔍 buscar", Action::SpotifySearch), ("⇄", Action::SpotifyShuffle(true)), ("⟲", Action::SpotifyRepeat(s.media.spotify.repeat.cycle()))] {
            let (x, y) = at(&f, label); assert_eq!(click(&f, x, y), Some(action));
        }
        let (x, y) = at(&f, "vol ");
        for cell in 0..VOLUME_CELLS { assert_eq!(click(&f, x + 4 + cell, y), Some(Action::SpotifyVolume(volume_percent(cell)))); }
        s.media.spotify.running = false; s.media.spotify.device_id = None;
        let f = frame(&s, 48, 0); at(&f, "sin dispositivo");
        for label in ["🔍 buscar", "vol ", "⇄", "⟲"] { let (x,y) = at(&f, label); assert_eq!(click(&f, x + usize::from(label == "vol ") * 4, y), None); }
        for w in 8..46 { assert!(spotify_card(&s, w, false).rows.iter().all(|r| width_of(r) <= w)); }
    }
    #[test]
    fn spotify_search_prompt_and_result_actions() {
        let mut s = sample(); s.ui.tab = Tab::Media; s.media.spotify.linked = true;
        let f = frame(&s, 48, 0); let (x, y) = at(&f, "🔍 buscar"); assert_eq!(click(&f, x, y), None);
        s.media.spotify.running = true; s.media.spotify.device_id = Some("dev".into());
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "🔍 buscar");
        assert_eq!(click(&f, x, y), Some(Action::SpotifySearch));
        apply(&mut s, Event::Ui(Action::SpotifySearch));
        for ch in "q17".chars() { apply(&mut s, Event::Key(Key::Char(ch))); }
        assert_eq!(s.ui.spotify_prompt.as_deref(), Some("q17")); assert_eq!(s.ui.tab, Tab::Media); assert!(!s.quit);
        let f = frame(&s, 42, 0); at(&f, "> q17▏");
        apply(&mut s, Event::Key(Key::Enter)); assert_eq!(s.ui.spotify_prompt, None); assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Search("q17".into())));
        s.ui.spotify_cmd = None; apply(&mut s, Event::Ui(Action::SpotifySearch)); apply(&mut s, Event::Key(Key::Esc)); assert!(s.ui.spotify_prompt.is_none() && s.ui.spotify_cmd.is_none());
        apply(&mut s, Event::Ui(Action::SpotifySearch)); apply(&mut s, Event::Key(Key::Enter)); assert!(s.ui.spotify_cmd.is_none());
        s.media.spotify.search_done = true;
        let f = frame(&s, 42, 0); at(&f, "sin resultados");
        s.media.spotify.results = (0..10).map(|i| crate::media::SearchTrack { id: format!("id{i}"), title: format!("Resultado {i}"), artist: "Artista".into(), cover: None }).collect();
        let f = frame(&s, 42, 0);
        for i in 0..10 { let (x,y) = at(&f, &format!("Resultado {i}")); assert_eq!(click(&f, x, y), Some(Action::SpotifyPlayResult(i))); }
        apply(&mut s, Event::Ui(Action::SpotifyPlayResult(9))); assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::PlayResult(9)));
    }
    #[test]
    fn transport_all_rendered_glyph_columns() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        s.media.spotify.linked = true; s.media.spotify.running = true; s.media.spotify.device_id = Some("dev".into());
        s.media.spotify.title = Some("Spotify track".into());
        s.media.youtube.title = Some("YouTube track".into());
        s.media.youtube.queue = vec![crate::media::Video { id: "v".into(), title: "YouTube track".into() }];
        for playing in [true, false] {
            s.media.spotify.playing = playing; s.media.youtube.playing = playing;
            let f = frame(&s, 46, 0);
            let rows: Vec<_> = f.lines.iter().enumerate().filter(|(_, l)| plain(l).contains("◀◀")).collect();
            assert_eq!(rows.len(), 2);
            for (i, (row, line)) in rows.into_iter().enumerate() {
                let text = plain(line);
                let middle = if playing { "❚❚" } else { "▶" };
                for (glyph, action) in [("◀◀", if i == 0 { Action::SpotifyPrevious } else { Action::PrevTrack }), (middle, if i == 0 { if playing { Action::SpotifyPause } else { Action::SpotifyResume } } else { Action::PlayPause }), ("▶▶", if i == 0 { Action::SpotifyNext } else { Action::NextTrack })] {
                    let start = text[..text.find(glyph).unwrap()].width();
                    for col in start..start + glyph.width() { assert_eq!(click(&f, col, row), Some(action.clone()), "{text} col {col}"); }
                }
            }
        }
    }

    #[test]
    fn spotify_transport_and_seek_columns() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        s.media.spotify.linked = true; s.media.spotify.running = true; s.media.spotify.device_id = Some("dev".into());
        s.media.spotify.title = Some("Tema único".into()); s.media.spotify.artist = "Artista".into();
        s.media.spotify.playing = true; s.media.spotify.position_ms = 30_000; s.media.spotify.duration_ms = 120_000;
        let f = frame(&s, 42, 0);
        let (x, y) = at(&f, "◀◀"); assert_eq!(click(&f, x, y), Some(Action::SpotifyPrevious));
        let (x, y) = at(&f, "❚❚   "); assert_eq!(click(&f, x, y), Some(Action::SpotifyPause));
        let (x, y) = at(&f, "▶▶"); assert_eq!(click(&f, x, y), Some(Action::SpotifyNext));
        let (_, bar) = at(&f, "━━━"); let inner = 42 - 2 - 4;
        for k in 0..inner { assert_eq!(click(&f, 3 + k, bar), Some(Action::SpotifySeek(k as f64 / (inner - 1) as f64))); }
        at(&f, "0:30 / 2:00"); at(&f, "Tema único"); at(&f, "Artista");
    }
    #[test]
    fn spotify_transport_actions_map_to_commands() {
        let mut s = sample();
        for (action, cmd) in [(Action::SpotifyNext, SpotifyCmd::Next), (Action::SpotifyPrevious, SpotifyCmd::Previous), (Action::SpotifySeek(0.25), SpotifyCmd::Seek(0.25))] {
            apply(&mut s, Event::Ui(action)); assert_eq!(s.ui.spotify_cmd, Some(cmd));
        }
    }
    #[test]
    fn media_tab_sits_before_config_and_keeps_spotify_above_youtube() {
        // S10: keys follow visual order, so MEDIA takes 6 and the CONFIG gear moves to 7.
        assert_eq!(Tab::ALL.len(), 7);
        assert_eq!((Tab::ALL[5], Tab::ALL[6]), (Tab::Media, Tab::Config));
        for (key, tab) in [('1', Tab::Main), ('2', Tab::Git), ('3', Tab::Prof), ('4', Tab::Mem), ('5', Tab::Tree), ('6', Tab::Media), ('7', Tab::Config)] {
            assert_eq!(Tab::from_key(key), Some(tab), "key {key}");
            assert_eq!(tab.key(), key);
        }
        // The new tab is addressable from the config file, and narrow names stay distinguishable.
        assert_eq!(Tab::from_id("media"), Some(Tab::Media));
        let mut short: Vec<&str> = Tab::ALL.iter().map(|t| t.name(true)).collect();
        short.sort(); let total = short.len(); short.dedup();
        assert_eq!(short.len(), total, "two-letter tab names collide: {short:?}");

        // S1: Spotify occupies the upper section and YouTube the section directly below it.
        let s = sample();
        let lines = on(&s, Tab::Media, 42);
        let spotify = lines.iter().position(|l| l.contains("Spotify")).expect("spotify section");
        let youtube = lines.iter().position(|l| l.contains("YouTube")).expect("youtube section");
        assert!(spotify < youtube, "Spotify must sit above YouTube: {lines:#?}");
        find(&lines, "falta el client_id");
        find(&lines, "developer.spotify.com");
        find(&lines, "sin reproducción");
        assert!(lines.iter().all(|l| l.width() <= 42), "no line may overflow 42 columns: {lines:#?}");

        // The bar shows the new tab, and clicking its name selects it.
        let f = frame(&s, 60, 0);
        let (x, y) = at(&f, "MEDIA");
        assert_eq!(click(&f, x, y), Some(Action::Tab(Tab::Media)));
        assert_eq!(click(&f, at(&frame(&s, 60, 0), CONFIG_ICON).0, y), Some(Action::Tab(Tab::Config)));
    }
    #[test]
    fn clock_and_seek_bar_are_width_exact() {
        assert_eq!(clock(0.0), "0:00");
        assert_eq!(clock(83.9), "1:23");
        assert_eq!(clock(-5.0), "0:00", "a negative position is drawn as zero");
        assert_eq!(clock(f64::NAN), "0:00");
        assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(plain(&seek_bar(0.0, 4)), "●┄┄┄");
        assert_eq!(plain(&seek_bar(0.5, 5)), "━━●┄┄");
        assert_eq!(plain(&seek_bar(1.0, 4)), "━━━●");
        assert!(seek_bar(0.5, 0).is_empty(), "no bar in a zero-width pane");
        for inner in 1..60 { assert_eq!(seek_bar(0.37, inner).iter().map(|g| g.text.width()).sum::<usize>(), inner); }
    }
    #[test]
    fn youtube_playlist_interactions_and_window() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        let f = frame(&s, 46, 0);
        let (x,y) = at(&f, "🔍 listas");
        act(&mut s.ui, click(&f,x,y).unwrap());
        assert_eq!(s.ui.yt_lists.as_deref(), Some("")); assert!(s.ui.prompt.is_none() && s.ui.spotify_prompt.is_none());
        key(&mut s, Key::Char('x')); key(&mut s, Key::Enter);
        assert_eq!(s.ui.media_cmd.take(), Some(MediaCmd::SearchPlaylists("x".into())));
        key(&mut s, Key::Enter); assert_eq!(s.ui.media_cmd, None);
        let f = frame(&s, 46, 0); let (x,y) = at(&f, "🔍 buscar"); act(&mut s.ui, click(&f,x,y).unwrap());
        assert!(s.ui.prompt.is_some() && s.ui.yt_lists.is_none()); s.ui.prompt = None;
        s.media.youtube.playlist_view = true;
        s.media.youtube.playlists = vec![crate::media::youtube::Playlist { id: "p1".into(), title: "Playlist Alpha".into() }, crate::media::youtube::Playlist { id: "p2".into(), title: "Playlist Beta".into() }];
        s.media.youtube.videos = (1..=25).map(|i| crate::media::Video { id: i.to_string(), title: format!("Video {i:02}") }).collect();
        let f = frame(&s, 46, 0); let (x,y) = at(&f, "Playlist Alpha");
        let lines: Vec<_> = f.lines.iter().map(|l| plain(l)).collect();
        for line in &lines { if line.contains("Playlist Alpha") || line.contains("Playlist Beta") || line.contains("⌁ Videos") || line.contains("Video 0") || line.contains("Video 10") { println!("{line}"); } }
        assert!(lines.iter().any(|l| l.contains("1-10/25")));
        for i in 1..=10 { assert!(lines.iter().any(|l| l.contains(&format!("{:>2}. Video {i:02}", i)))); }
        assert!(!lines.iter().any(|l| l.contains("Video 11")));
        assert_eq!(click(&f,x,y), Some(Action::YtOpenPlaylist("p1".into())));
        let (_,y) = at(&f, "Video 01"); assert_eq!(wheel(&f,y,true), Some(Action::ScrollVideos(3)));
        let old = s.ui.scroll; act(&mut s.ui, wheel(&f,y,true).unwrap()); assert_eq!(s.ui.scroll, old);
        let f = frame(&s, 46, 0); let lines: Vec<_> = f.lines.iter().map(|l| plain(l)).collect();
        assert!(lines.iter().any(|l| l.contains("4-13/25")));
        let (x,y) = at(&f, "Video 12"); assert_eq!(click(&f,x,y), Some(Action::YtPlayVideo(11)));
        s.media.youtube.queue = vec![s.media.youtube.videos[11].clone()]; s.media.youtube.from_playlist = true;
        let f = frame(&s, 46, 0); assert!(plain(&f.lines[at(&f, "Video 12").1]).contains("▶"));
        // A mix (`buscar`) lists its own recommendations and marks the same way; only the title
        // changes, because the source is different.
        s.media.youtube.from_playlist = false; let f = frame(&s, 46, 0);
        assert!(plain(&f.lines[at(&f, "Video 12").1]).contains("▶"), "the mark follows the queue in both sources");
        assert!(f.lines.iter().any(|l| plain(l).contains("⌁ Sugeridos")), "a mix names its source");
        s.ui.video_scroll = 9; act(&mut s.ui, Action::YtOpenPlaylist("p2".into())); assert_eq!(s.ui.video_scroll, 0);
    }
    #[test]
    fn track_search_hides_playlist_videos() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        s.media.youtube.playlists.push(crate::media::youtube::Playlist { id: "p".into(), title: "Old playlist".into() });
        s.media.youtube.videos.push(crate::media::Video { id: "v".into(), title: "Old video".into() });
        act(&mut s.ui, Action::Search);
        let lines = render_lines(&s, 46);
        assert!(!lines.iter().any(|l| l.contains("Old playlist") || l.contains("⌁ Videos") || l.contains("Old video")), "{lines:?}");
    }

    #[test]
    fn youtube_playlist_ui_contract() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        act(&mut s.ui, Action::YtLists);
        assert_eq!(s.ui.yt_lists.as_deref(), Some(""));
        key(&mut s, Key::Char('a')); key(&mut s, Key::Enter);
        assert_eq!(s.ui.media_cmd, Some(MediaCmd::SearchPlaylists("a".into())));
        assert!(s.ui.prompt.is_none());
        s.media.youtube.videos = (1..=25).map(|i| crate::media::Video { id: i.to_string(), title: format!("Video {i}") }).collect();
        let f = frame(&s, 46, 0);
        assert!(f.lines.iter().map(|l| plain(l)).any(|l| l.contains("1-10/25")));
        act(&mut s.ui, Action::YtOpenPlaylist("list".into()));
        assert_eq!(s.ui.media_cmd, Some(MediaCmd::OpenPlaylist("list".into())));
    }
    #[test]
    fn youtube_transport_and_every_seek_column_click_their_own_cell() {
        let mut s = sample();
        s.ui.tab = Tab::Media;
        s.media.youtube = crate::media::Youtube { state: "reproduciendo".into(), title: Some("Daft Punk - Around The World".into()),
            position: 30.0, duration: 242.0, playing: true,
            queue: vec![crate::media::Video { id: "a".into(), title: "A".into() }, crate::media::Video { id: "b".into(), title: "B".into() }], index: 0, ..crate::media::Youtube::default() };
        let f = frame(&s, 42, 0);
        let (x, y) = at(&f, "◀◀"); assert_eq!(click(&f, x, y), Some(Action::PrevTrack));
        let (x, y) = at(&f, "❚❚"); assert_eq!(click(&f, x, y), Some(Action::PlayPause));
        let (x, y) = at(&f, "▶▶"); assert_eq!(click(&f, x, y), Some(Action::NextTrack));
        // S5: the bar seeks where it is clicked, and its marker sits at position/duration.
        let (mark, bar) = at(&f, "●");
        // The card is two columns narrower than the frame, and its inner text starts at column 3.
        let inner = 42 - 2 - 4; let start = 3;
        assert_eq!(click(&f, start, bar), Some(Action::Seek(0.0)));
        assert_eq!(click(&f, start + inner - 1, bar), Some(Action::Seek(1.0)), "the last column is the end");
        match click(&f, mark, bar) { Some(Action::Seek(frac)) => assert!((frac - 30.0 / 242.0).abs() < 0.02, "marker at {frac}"), other => panic!("no seek at the marker: {other:?}") }
        assert!(plain(&f.lines[bar + 1]).contains("0:30 / 4:02"), "the elapsed and total time are shown: {:?}", plain(&f.lines[bar + 1]));
        at(&f, "1/2"); // queue position
    }
    #[test]
    fn youtube_search_box_is_modal_and_submits_exactly_one_play_command() {
        let mut s = sample();
        apply(&mut s, Event::Ui(Action::Search)); assert_eq!(s.ui.prompt.as_deref(), Some(""));
        for c in "daft punk".chars() { apply(&mut s, Event::Key(Key::Char(c))); }
        assert_eq!(s.ui.prompt.as_deref(), Some("daft punk"));
        assert!(!s.quit, "q is text while the search box is open"); assert_eq!(s.ui.tab, Tab::Main, "tab keys are text too");
        apply(&mut s, Event::Key(Key::Back)); apply(&mut s, Event::Key(Key::Char('k')));
        apply(&mut s, Event::Key(Key::Enter));
        assert_eq!(s.ui.prompt, None);
        assert_eq!(s.ui.media_cmd, Some(MediaCmd::Play("daft punk".into(), None)));
        // Esc closes without playing, and an empty query never reaches the engine.
        s.ui.media_cmd = None;
        apply(&mut s, Event::Ui(Action::Search)); apply(&mut s, Event::Key(Key::Char('x'))); apply(&mut s, Event::Key(Key::Esc));
        assert!(s.ui.prompt.is_none() && s.ui.media_cmd.is_none(), "Esc closes and plays nothing");
        apply(&mut s, Event::Ui(Action::Search)); apply(&mut s, Event::Key(Key::Enter));
        assert_eq!(s.ui.media_cmd, None, "an empty query is not a search");
    }
    #[test]
    fn spotify_oauth_url_is_fully_visible_across_card_rows() {
        let mut s = sample(); s.ui.tab = Tab::Media; s.media.spotify.linked = true;
        let url = format!("https://accounts.spotify.com/authorize?{}", "abcdefghij".repeat(12));
        s.media.spotify.oauth_url = Some(url.clone()); s.media.spotify.state = url.clone();
        let f = frame(&s, 42, 0);
        let lines: Vec<String> = f.lines.iter().map(|line| plain(line)).collect();
        for chunk in url.as_bytes().chunks(30) { let chunk = std::str::from_utf8(chunk).unwrap(); assert!(lines.iter().any(|line| line.contains(chunk)), "missing {chunk}"); }
    }
    #[test]
    fn spotify_playback_rows_explain_prerequisites_and_map_commands() {
        let mut s = sample(); s.ui.tab = Tab::Media; s.media.spotify.linked = true;
        let f = frame(&s, 42, 0);
        at(&f, "▶ dispositivo"); at(&f, "sin dispositivo");
        let (x, y) = at(&f, "▶ dispositivo");
        assert_eq!(click(&f, x, y), Some(Action::SpotifyStart));
        apply(&mut s, Event::Ui(Action::SpotifyStart)); assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Start));
        s.media.spotify.running = true; s.media.spotify.device_id = Some("dev".into());
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "reproducir lista"); assert_eq!(click(&f, x, y), None);
        s.media.spotify.playlists.push(crate::media::Playlist { id: "p".into(), name: "Rock".into(), cover: None });
        s.media.spotify.selected = Some(0); s.media.spotify.tracks.push(crate::media::Track { id: "t1".into(), title: "Tema".into(), artist: "Artista".into(), position: 0 });
        let f = frame(&s, 42, 0);
        let (x, y) = at(&f, "reproducir lista"); assert_eq!(click(&f, x, y), Some(Action::SpotifyPlay(0)));
        apply(&mut s, Event::Ui(Action::SpotifyPlay(0))); assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Play(0)));
        s.media.spotify.playing = true;
        let f = frame(&s, 42, 0);
        let (x, y) = at(&f, "pausar"); assert_eq!(click(&f, x, y), Some(Action::SpotifyPause));
        apply(&mut s, Event::Ui(Action::SpotifyPause)); assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Pause));
        let (x, y) = at(&f, "Tema"); assert_eq!(click(&f, x, y), Some(Action::SpotifyPlay(0)));
    }
    #[test]
    fn spotify_card_states_the_prerequisite_then_offers_a_renewal() {
        let mut s = sample();
        s.ui.tab = Tab::Media;
        let f = frame(&s, 42, 0);
        at(&f, "falta el client_id");
        at(&f, "developer.spotify.com");
        at(&f, "~/.config/gs3/spotify.json");
        // The redirect is the contract: the app must allowlist exactly this loopback URL.
        at(&f, "http://127.0.0.1:8899/callback");
        let (x, y) = at(&f, "conectar");
        assert_eq!(click(&f, x, y), Some(Action::SpotifyAuthorize), "the row starts the consent");
        apply(&mut s, Event::Ui(Action::SpotifyAuthorize));
        assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Authorize));
        // With a stored session the setup rows are gone and only the renewal is offered.
        s.media.spotify = crate::media::Spotify { state: "sesión guardada".into(), linked: true, ..Default::default() };
        s.ui.spotify_cmd = None;
        let f = frame(&s, 42, 0);
        at(&f, "sesión guardada");
        assert!(!f.lines.iter().any(|l| plain(l).contains("conectar")), "no setup rows once linked");
        assert!(!f.lines.iter().any(|l| plain(l).contains("client_id")), "the prerequisite is resolved");
        let (x, y) = at(&f, "renovar");
        assert_eq!(click(&f, x, y), Some(Action::SpotifyRefresh));
        apply(&mut s, Event::Ui(Action::SpotifyRefresh));
        assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Refresh));
    }
    #[test]
    fn spotify_card_lists_own_playlists_and_the_playlist_rows() {
        let mut s = sample();
        s.ui.tab = Tab::Media;
        let playlist = |id: &str, name: &str| crate::media::Playlist { id: id.into(), name: name.into(), cover: None };
        let track = |title: &str| crate::media::Track { id: title.into(), title: title.into(), artist: "Daft Punk".into(), position: 0 };
        s.media.spotify = crate::media::Spotify {
            state: "2 listas".into(),
            linked: true,
            playlists: vec![playlist("p1", "Rock"), playlist("p2", "Jazz")],
            selected: Some(1),
            tracks: (0..10).map(|i| track(&format!("Tema {i}"))).collect(),
            ..Default::default()
        };
        let f = frame(&s, 42, 0);
        at(&f, "2 listas");
        // S2: the playlists are the user's own, and clicking one asks for its tracks.
        let (x, y) = at(&f, "Rock");
        assert_eq!(click(&f, x, y), Some(Action::SelectPlaylist("p1".into())));
        let (x, y) = at(&f, "Jazz");
        assert_eq!(click(&f, x, y), Some(Action::SelectPlaylist("p2".into())));
        apply(&mut s, Event::Ui(Action::SelectPlaylist("p2".into())));
        assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Select("p2".into())));
        // The window holds ten rows, so a ten-track list is shown whole.
        let rows = f.lines.iter().filter(|l| plain(l).contains("Tema ")).count();
        assert_eq!(rows, 10, "ten track rows, one per song: {:#?}", f.lines.iter().map(|l| plain(l)).collect::<Vec<_>>());
        at(&f, "10. Tema 9");
        assert!(f.lines.iter().any(|l| plain(l).contains("Daft Punk")), "each row keeps its artist");
        assert!(f.lines.iter().all(|l| plain(l).width() <= 42), "no line may overflow 42 columns");
        // The section keeps the YouTube control below the whole Spotify block.
        let spotify = f.lines.iter().position(|l| plain(l).contains("Spotify")).unwrap();
        let pistas = f.lines.iter().position(|l| plain(l).contains("Pistas")).unwrap();
        let youtube = f.lines.iter().position(|l| plain(l).contains("YouTube")).unwrap();
        assert!(spotify < pistas && pistas < youtube, "S1: Spotify above, YouTube below");
        // An empty profile says so instead of showing a blank card.
        s.media.spotify = crate::media::Spotify { state: "sin listas propias".into(), linked: true, ..Default::default() };
        let f = frame(&s, 42, 0);
        at(&f, "sin listas propias todavía");
        let (x, y) = at(&f, "traer mis listas");
        assert_eq!(click(&f, x, y), Some(Action::SpotifyPlaylists));
    }

    /// A playlist longer than the card's window: the `S13` fixture, where ten rows used to be all
    /// there was and the wheel had nothing left to reach.
    fn long_playlist(s: &mut State, n: usize) {
        s.ui.tab = Tab::Media;
        s.media.spotify = crate::media::Spotify {
            state: format!("{n} de Rock"),
            linked: true,
            running: true,
            device_id: Some("d".into()),
            playlists: vec![crate::media::Playlist { id: "p1".into(), name: "Rock".into(), cover: None }],
            selected: Some(0),
            tracks: (0..n).map(|i| crate::media::Track { id: format!("id{i}"), title: format!("Tema {i}"), artist: "Daft Punk".into(), position: i }).collect(),
            ..Default::default()
        };
    }

    #[test]
    fn a_long_playlist_scrolls_inside_its_own_card() {
        let mut s = sample();
        long_playlist(&mut s, 57);
        let f = frame(&s, 42, 40);
        let visible = |f: &Frame| f.lines.iter().map(|l| plain(l)).collect::<Vec<_>>();
        // Ten rows fit the window; the rest wait for the list to scroll, not for the body.
        at(&f, "10. Tema 9");
        assert!(!visible(&f).iter().any(|l| l.contains(" 11. Tema 10")), "the eleventh row is out of the window");
        assert!(visible(&f).iter().any(|l| l.contains("Pistas") && l.contains("1-10/57")), "the title says which rows are on screen: {:?}", visible(&f));
        // The wheel over the card belongs to the list, and the body stays where it was.
        let (_, y) = at(&f, "Tema 3");
        let step = wheel(&f, y, true).expect("the wheel over the list scrolls the list");
        assert_eq!(step, Action::ScrollList(3));
        apply(&mut s, Event::Ui(step));
        assert_eq!(s.ui.scroll[Tab::Media.index()], 0, "the body does not move under the list");
        let f = frame(&s, 42, 40);
        let scrolled = visible(&f);
        assert!(!scrolled.iter().any(|l| l.contains(" 1. Tema 0") || l.contains(" 3. Tema 2")), "the first rows left the window: {scrolled:#?}");
        assert!(scrolled.iter().any(|l| l.contains(" 4. Tema 3")), "the window starts at the fourth track: {scrolled:#?}");
        assert!(scrolled.iter().any(|l| l.contains("13. Tema 12")), "and reaches one row further: {scrolled:#?}");
        assert!(scrolled.iter().any(|l| l.contains("Pistas") && l.contains("4-13/57")), "the title moved with it");
        // A click plays the row it names, not the position inside the window.
        let (x, y) = at(&f, "Tema 12");
        assert_eq!(click(&f, x, y), Some(Action::SpotifyPlay(12)));
    }

    #[test]
    fn the_track_list_clamps_at_its_end_and_walks_back() {
        let mut s = sample();
        long_playlist(&mut s, 57);
        // A stored scroll past the end is clamped, so the last rows stay reachable.
        s.ui.track_scroll = 1000;
        let f = frame(&s, 42, 40);
        let visible = |f: &Frame| f.lines.iter().map(|l| plain(l)).collect::<Vec<_>>();
        at(&f, "57. Tema 56");
        assert!(visible(&f).iter().any(|l| l.contains("Pistas") && l.contains("48-57/57")));
        // One step up walks back inside the list, not through the body.
        let (_, y) = at(&f, "Tema 50");
        let step = wheel(&f, y, false).expect("the wheel up over the list moves the list");
        assert_eq!(step, Action::ScrollList(44));
        apply(&mut s, Event::Ui(step));
        assert!(render_lines(&s, 42).iter().any(|l| l.contains(" 45. Tema 44")), "the window kept the list's scroll");
        // Past its own end the card hands the wheel back, or the YouTube section would be unreachable.
        s.ui.track_scroll = 1000;
        let f = frame(&s, 42, 30);
        assert!(f.max_scroll >= 3, "the fixture leaves the body scrollable: {}", f.max_scroll);
        let (_, y) = at(&f, "Pistas");
        assert_eq!(wheel(&f, y, true), Some(Action::ScrollTo(3)), "a list at its last row lets the body take the wheel");
    }

    #[test]
    fn a_row_outside_the_list_keeps_the_body_scroll() {
        let mut s = sample();
        long_playlist(&mut s, 57);
        let f = frame(&s, 42, 30);
        assert!(f.max_scroll >= 3, "the fixture leaves the body scrollable: {}", f.max_scroll);
        let (_, y) = at(&f, "vol ");
        assert_eq!(wheel(&f, y, true), Some(Action::ScrollTo(3)), "a Spotify control row is not a list");
    }

    #[test]
    fn opening_another_playlist_returns_the_list_to_its_first_rows() {
        let mut s = sample();
        long_playlist(&mut s, 57);
        s.ui.track_scroll = 30;
        apply(&mut s, Event::Ui(Action::SelectPlaylist("p1".into())));
        assert_eq!(s.ui.track_scroll, 0, "a new list starts at its first row");
        assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Select("p1".into())));
    }
    #[test]
    fn plain_controls_preserve_states_layout_and_actions() {
        for online in [false, true] { for mute in [false, true] { for deaf in [false, true] {
            let mut s = sample();
            s.ui.config = Config::from_json(&serde_json::json!({"plain_controls": true}));
            s.voice = Voice { online, mute, deaf };
            let labels = [if mute || deaf { "MUTE" } else { "MIC" }, if deaf { "DEAF" } else { "HP" }, "APP", "CAP"];
            for width in [32, 42, 60] {
                let f = frame(&s, width, 0);
                let (_, y) = at(&f, labels[0]);
                assert_eq!(y, f.tab_row + 2);
                assert_eq!(at(&f, "Status").1, y + 1);
                assert!(f.lines[y].iter().all(|g| g.text.is_ascii()), "controls must not need private-use glyphs");
                for (i, action) in [Action::VoiceMute, Action::VoiceDeaf, Action::VoiceOpen, Action::Screenshot].into_iter().enumerate() {
                    let (x, row) = at(&f, labels[i]); assert_eq!(row, y);
                    let slot = x - (5 - labels[i].width()) / 2;
                    for col in slot - 1..slot + 6 { assert_eq!(click(&f, col, y), Some(action.clone())); }
                    let active = if i == 0 { mute || deaf } else { deaf };
                    let expected = if i == 3 { Tone::Secondary } else if !online { Tone::Dim } else if i == 2 { Tone::Secondary } else if active { Tone::Coral } else { Tone::Mint };
                    let g = f.lines[y].iter().find(|g| g.text == labels[i]).unwrap();
                    assert_eq!((g.tone, g.bold), (expected, false));
                }
                assert!(f.lines.iter().all(|l| width_of(l) <= usize::from(width)));
            }
            let narrow = render_lines(&s, 28);
            assert!(!narrow.iter().any(|l| l.contains("CAP")));
            assert!(narrow.iter().any(|l| l.contains("APP")));
            for (action, command) in [(Action::VoiceMute, Some(VoiceCmd::Mute)), (Action::VoiceDeaf, Some(VoiceCmd::Deaf)), (Action::VoiceOpen, Some(VoiceCmd::Open)), (Action::Screenshot, None)] {
                s.ui.voice_cmd = None; s.ui.screenshot = false;
                assert!(apply(&mut s, Event::Ui(action)));
                assert_eq!(s.ui.voice_cmd, command);
                assert_eq!(s.ui.screenshot, command.is_none());
                assert!(!s.ui.save && !s.ui.open_picker);
            }
        }}}
    }
    #[test]
    fn nerd_font_controls_preserve_states_colors_and_clicks() {
        let mut s = sample();
        for online in [false, true] { for mute in [false, true] { for deaf in [false, true] {
            s.voice = Voice { online, mute, deaf };
            let f = frame(&s, 42, 0);
            let icons = [if mute || deaf { MIC_OFF } else { MIC_ICON }, if deaf { HEADPHONES_OFF } else { HEADPHONES_ICON }, DISCORD_ICON];
            assert!(f.lines.iter().any(|line| plain(line).contains('\u{f066f}')), "voice launcher must use the Discord logo");
            let (bx, y) = at(&f, icons[0]);
            assert_eq!(y, f.tab_row + 2);
            assert_eq!(at(&f, "Status").1, y + 1);
            for (i, action) in [Action::VoiceMute, Action::VoiceDeaf, Action::VoiceOpen].into_iter().enumerate() {
                let (x, row) = at(&f, icons[i]);
                assert_eq!(row, y);
                let g = f.lines[y].iter().find(|g| g.text == icons[i]).unwrap();
                let active = if i == 0 { mute || deaf } else { deaf };
                let expected = if !online { Tone::Dim } else if i == 2 { Tone::Secondary } else if active { Tone::Coral } else { Tone::Mint };
                assert_eq!((g.tone, g.bold), (expected, false));
                for col in x - 1..x + icons[i].width() + 1 { assert_eq!(click(&f, col, y), Some(action.clone())); }
                s.ui.voice_cmd = None;
                assert!(apply(&mut s, Event::Ui(action)));
                assert_eq!(s.ui.voice_cmd, Some([VoiceCmd::Mute, VoiceCmd::Deaf, VoiceCmd::Open][i]));
                assert!(!s.ui.save);
            }
            assert_eq!(click(&f, bx, y - 1), None);
        }}}
        for t in [Tab::Git, Tab::Prof, Tab::Mem, Tab::Tree, Tab::Config] {
            assert!(!on(&s, t, 42).iter().any(|l| l.contains(DISCORD_ICON)));
        }
    }
    #[test]
    fn capture_icon_preserves_centered_group_and_action() {
        let mut s = sample();
        let f = frame(&s, 42, 0);
        let (x, y) = at(&f, CAPTURE_ICON);
        assert_eq!(x, at(&f, DISCORD_ICON).0 + 8);
        for width in [32, 42, 60] {
            let group = frame(&s, width, 0);
            let positions: Vec<_> = [MIC_ICON, HEADPHONES_ICON, DISCORD_ICON, CAPTURE_ICON].iter().map(|icon| at(&group, icon).0).collect();
            assert!(positions.windows(2).all(|pair| pair[1] - pair[0] == 8));
            let left = positions[0];
            let right = usize::from(width) - positions[3] - CAPTURE_ICON.width();
            assert!(left.abs_diff(right) <= 1, "group must be centered at width {width}");
        }
        let g = f.lines[y].iter().find(|g| g.text == CAPTURE_ICON).unwrap();
        assert_eq!((g.tone, g.bold), (Tone::Secondary, false));
        for col in x - 1..x + CAPTURE_ICON.width() + 1 { assert_eq!(click(&f, col, y), Some(Action::Screenshot)); }
        assert!(apply(&mut s, Event::Ui(Action::Screenshot)));
        assert!(s.ui.screenshot && !s.ui.save && s.ui.voice_cmd.is_none());
        let narrow = render_lines(&s, 28);
        assert!(!narrow.iter().any(|l| l.contains(CAPTURE_ICON)));
        assert!(narrow.iter().any(|l| l.contains(DISCORD_ICON)));
        for width in [28, 32, 42, 60] {
            assert!(render_lines(&s, width).iter().all(|l| l.width() <= usize::from(width)));
        }
        for t in [Tab::Git, Tab::Prof, Tab::Mem, Tab::Tree, Tab::Config] {
            assert!(!on(&s, t, 42).iter().any(|l| l.contains(CAPTURE_ICON)));
        }
    }
    #[test]
    fn model_picker_clicks_every_model_row() {
        let mut s = sample();
        for needle in ["G P T - 6 . 1 - S O L", "◆ MEDIUM ◆"] {
            s.ui.open_picker = false;
            let f = frame(&s, 42, 0); let (x, y) = at(&f, needle);
            assert_eq!(click(&f, x, y), Some(Action::ModelPicker));
            assert!(apply(&mut s, Event::Ui(click(&f, x, y).unwrap())));
            assert!(s.ui.open_picker); assert!(!s.ui.save);
        }
        s.session.model.clear();
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "sin modelo");
        assert_eq!(click(&f, x, y), Some(Action::ModelPicker));
        s.session.model = "provider/long-model-name-that-wraps-across-rows".into();
        let f = frame(&s, 24, 0);
        let first = at(&f, "LONG-MODEL").1;
        for y in first..at(&f, "MEDIUM").1 + 1 {
            assert_eq!(click(&f, 3, y), Some(Action::ModelPicker));
        }
        // Existing title collapse is still separate, and collapsed rows have no picker hits.
        apply(&mut s, Event::Ui(Action::Collapse("status".into())));
        let f = frame(&s, 42, 0);
        assert!(!f.hits.iter().any(|h| h.action == Action::ModelPicker));
        let (x, y) = at(&f, "Status");
        assert_eq!(click(&f, x, y), Some(Action::Collapse("status".into())));
    }
    #[test]
    fn banner_placeholder_name_and_tab_bar() {
        let mut s = sample(); let l = render_lines(&s, 60);
        assert_eq!(l[0], ""); assert!(l[1].contains("⌁ - - ⌁"), "{l:#?}"); assert!(l[2].trim() == "GENTLE-SHELL");
        assert_eq!(l[2].find('G').unwrap(), 1 + (58 - 12) / 2, "centered in the padded rail");
        // Wide: `[ MAIN ]` active, other tabs by name (no numbers), CONFIG is the gear, git sync badge, gaps spread evenly.
        let tabs = &l[4]; assert!(tabs.starts_with(" [ MAIN ]") && tabs.contains("GIT ▲1") && tabs.trim_end().ends_with('⛭'), "{tabs:?}");
        assert_eq!(tabs.split_whitespace().collect::<Vec<_>>(), ["[", "MAIN", "]", "GIT", "▲1", "PROF", "MEM", "TREE", "MEDIA", "⛭"]);
        assert_eq!(tabs.width(), 59); assert!(l[5].trim().chars().all(|c| c == '─') && l[5].width() == 59); assert!(l[6].contains(MIC_ICON), "MAIN: icons right under the divider");
        for t in [Tab::Git, Tab::Prof, Tab::Mem, Tab::Tree, Tab::Media, Tab::Config] { let mut o = s.clone(); o.ui.tab = t; let f = frame(&o, 60, 0); assert!(f.lines[f.tab_row + 2].is_empty() || plain(&f.lines[f.tab_row + 2]).trim().is_empty(), "{t:?} keeps its blank line"); }
        // Under 44 columns: compact `[MAIN]`; hidden tabs leave no gap.
        let bar = |s: &State, w: u16| render_lines(s, w)[4].split_whitespace().map(String::from).collect::<Vec<_>>();
        assert_eq!(bar(&s, 42), ["[MAIN]", "GIT", "▲1", "PROF", "MEM", "TREE", "MEDIA", "⛭"]); assert_eq!(bar(&s, 37), bar(&s, 42), "the real 37-col pane keeps full names");
        s.git.ahead_behind = None; s.ui.config.hidden = vec![Tab::Tree]; assert_eq!(bar(&s, 42), ["[MAIN]", "GIT", "PROF", "MEM", "MEDIA", "⛭"]);
        assert!(!render_lines(&s, 42)[4].chars().any(|c| c.is_ascii_digit()), "no tab numbers");
        s.git.ahead_behind = Some((1, 0)); s.ui.config.hidden.clear();
        // Too narrow for the names: two-letter names, still one per tab and clickable.
        assert_eq!(bar(&s, 30), ["[MA]", "GI", "▲1", "PR", "ME", "TR", "MD", "⛭"]);
        s.ui.tab = Tab::Config; assert_eq!(bar(&s, 42).last().unwrap(), "[⛭]"); s.ui.tab = Tab::Main;
        // S11: CONFIG tab is the gearbox `⛭`, banner and card titles the power mark `⌁`; both purple, 1 column, no `⚙` left.
        let f = frame(&s, 42, 0); let icon = |c: char| f.lines.iter().flatten().filter(|g| g.text.contains(c)).cloned().collect::<Vec<_>>();
        let (tab, mark) = (icon('⛭'), icon('⌁')); assert!(tab.len() == 1 && mark.len() >= 4, "{tab:?} {mark:?}");
        assert!(tab.iter().chain(&mark).all(|g| (g.text == "⛭" || g.text == "⌁") && g.tone == Tone::Purple && g.text.width() == 1));
        assert_eq!(icon('⚙').len(), 0, "the voice gear is pixel art now"); assert!(f.lines[f.tab_row].iter().any(|g| g.text == "⛭"));
        assert!(f.lines.iter().flatten().any(|g| g.text == " Status" && g.tone == Tone::PinkBright));
        s.ui.config.name = "RamonsDev".into(); let l = render_lines(&s, 42);
        assert!(l[1].contains("⌁ RamonsDev ⌁") && !l[1].contains("- -"));
        for w in [0u16, 1, 5, 20, 42, 80] { for t in Tab::ALL { for l in on(&s, t, w) { assert!(l.width() <= w as usize, "{w} {l:?}"); } } }
    }
    #[test]
    fn cards_use_the_double_frame_full_width() {
        let s = sample();
        for t in Tab::ALL { for l in on(&s, t, 42).iter().skip(7).filter(|l| !l.trim().is_empty() && !l.contains("Sin información") && !l.contains(MIC_ICON)) {
            assert_eq!(l.width(), 41, "{l:?}"); let c = l.trim_start().chars().next().unwrap(); let z = l.chars().last().unwrap();
            assert!(matches!((c, z), ('╔', '╗') | ('║', '║') | ('╠', '╣') | ('╚', '╝')), "{l:?}");
        } }
        let l = on(&s, Tab::Main, 42);
        assert!(find(&l, "⌁ Status").starts_with(" ╔═ ⌁ Status ═") && find(&l, "⌁ Status").ends_with(" [-] ═╗"));
        assert!(find(&l, "Project").contains("/srv/repo")); assert!(find(&l, "Branch").contains("⎇ feat/x ±2"));
        // Model: its own section between dividers, spaced capitals (no provider) and `◆ MEDIUM ◆`, both centered.
        assert!(!l.iter().any(|x| x.contains("Model") || x.contains("CPAMC")));
        let i = l.iter().position(|x| x.contains("G P T - 6 . 1 - S O L")).unwrap();
        assert!(l[i - 1].contains("╠═") && l[i + 1].contains("◆ MEDIUM ◆") && l[i + 2].contains("╠═"), "{:#?}", &l[i - 1..i + 3]);
        for r in [&l[i], &l[i + 1]] { let inner = r.trim_start().trim_start_matches('║').trim_end().trim_end_matches('║'); let (a, b) = (inner.len() - inner.trim_start().len(), inner.len() - inner.trim_end().len()); assert!(a.abs_diff(b) <= 1, "centered {r:?}"); }
        let f = frame(&s, 42, 0); let tone = |t: &str| f.lines.iter().flatten().find(|g| g.text.contains(t)).map(|g| (g.tone, g.bold));
        assert_eq!([tone("G P T"), tone("MEDIUM")], [Some((Tone::PinkBright, true)), Some((Tone::Gold, false))]);
        // Too long to space out: plain capitals, wrapped inside the card; no thinking (or off): no ◆ row; no model: dim placeholder.
        let mut m = s.clone(); m.session.model = "cpamc/claude-opus-4-1-20250805-extended".into(); m.session.thinking = "off".into();
        let l = render_lines(&m, 30); let rows: Vec<String> = l.iter().filter(|x| x.contains("CLAUDE") || x.contains("EXTENDED")).map(|x| x.trim().trim_matches('║').trim().to_string()).collect();
        assert_eq!(rows.concat(), "CLAUDE-OPUS-4-1-20250805-EXTENDED"); assert!(rows.len() == 2 && !l.iter().any(|x| x.contains('◆')), "{l:#?}");
        m.session.model.clear(); find(&render_lines(&m, 42), "sin modelo");
        // A fresh session (no assistant turn yet) shows the host defaults from settings.json, like the gs2 footer.
        m.session.thinking.clear(); m.roster = roster_sample().roster; let l = render_lines(&m, 42); find(&l, "G P T - 6 . 1 - S O L"); find(&l, "◆ MEDIUM ◆");
        for w in [20u16, 30, 36, 42, 80] { for x in render_lines(&s, w) { assert!(x.width() <= w as usize, "{w} {x:?}"); } }
        let l = on(&s, Tab::Main, 42);
        assert!(find(&l, "● working").contains("—")); assert!(find(&l, "herdr ● w1:p1").contains("conectado"));
        assert!(find(&l, "33.7k / 370k tokens").contains("● Óptimo 9.1%"));
        let gauge = find(&l, "▰"); assert_eq!(gauge.matches('▰').count() + gauge.matches('▱').count(), 36);
        assert!(find(&l, "▲ 33.7k in").contains("▼ 14.4k out") && find(&l, "▲ 33.7k in").contains("Cost $0.250"));
        find(&on(&s, Tab::Tree, 42), "Escaneando…");
    }
    fn tree_sample() -> State {
        let mut s = git_sample(); s.ui.tab = Tab::Tree;
        let n = |rel: &str, dir: bool, count: usize, children: Vec<TreeNode>| TreeNode { name: rel.rsplit('/').next().unwrap().into(), rel: rel.into(), dir, count, children };
        s.tree = Some(Tree { project: "repo".into(), open: vec!["src".into()], error: None, nodes: vec![
            n("docs", true, 2, vec![]), n("src", true, 2, vec![n("src/a.rs", false, 0, vec![]), n("src/lib.rs", false, 0, vec![])]),
            n("Cargo.toml", false, 0, vec![]), n("new.txt", false, 0, vec![]), n("README.md", false, 0, vec![])] });
        s.ui.tree_open = vec!["src".into()]; s
    }
    #[test]
    fn tree_tab_shows_project_tree_with_git_badges() {
        let mut s = tree_sample(); let l = render_lines(&s, 42);
        assert!(find(&l, "⌁ PROYECTO · repo").contains("[ ⊟ ] [-] ═╗"));
        assert!(find(&l, "├── 📁 docs (2 items)").ends_with("  ║")); assert!(find(&l, "├── 📂 src (2 items)").ends_with("● ║"));
        assert!(find(&l, "│   ├── 📄 a.rs").ends_with("M ║")); assert!(find(&l, "│   └── 📄 lib.rs").ends_with("  ║"));
        find(&l, "├── 🔧 Cargo.toml"); assert!(find(&l, "├── 📝 new.txt").ends_with("? ║")); find(&l, "└── 📝 README.md");
        let f = frame(&s, 42, 0); let tone = |needle: &str| f.lines.iter().flatten().find(|g| g.text == needle).map(|g| (g.tone, g.bold));
        assert_eq!([tone("src"), tone("docs"), tone("a.rs"), tone("lib.rs"), tone("Cargo.toml"), tone("new.txt"), tone("├── ")],
            [Some((Tone::Yellow, false)), Some((Tone::Cyan, false)), Some((Tone::Yellow, false)), Some((Tone::Mint, false)), Some((Tone::Gold, false)), Some((Tone::Pink, false)), Some((Tone::Dim, false))]);
        // Folder click toggles it; a stale scan (old open set) is dropped, the matching one is kept.
        let (x, y) = at(&f, "docs"); let a = click(&f, x, y).unwrap(); assert_eq!(a, Action::Dir("docs".into()));
        apply(&mut s, Event::Ui(a)); assert_eq!(s.ui.tree_open, ["docs", "src"]);
        let old = s.tree.clone().unwrap(); let mut t = old.clone(); t.nodes.clear();
        assert!(!apply(&mut s, Event::Tree(t.clone()))); assert_eq!(s.tree.as_ref(), Some(&old));
        t.open = s.ui.tree_open.clone(); assert!(apply(&mut s, Event::Tree(t))); find(&render_lines(&s, 42), "Sin archivos en el proyecto · ⌁");
        // A changed file opens its diff in GIT; an unchanged one is only selected (bold pink).
        let mut s = tree_sample(); let f = frame(&s, 42, 0);
        let (x, y) = at(&f, "a.rs"); let a = click(&f, x, y).unwrap(); assert_eq!(a, Action::Inspect("src/a.rs".into()));
        apply(&mut s, Event::Ui(a)); assert_eq!((s.ui.tab, s.ui.git_file.as_deref(), s.ui.git_branch.as_deref()), (Tab::Git, Some("src/a.rs"), None));
        let mut s = tree_sample(); let f = frame(&s, 42, 0);
        let (x, y) = at(&f, "lib.rs"); let a = click(&f, x, y).unwrap(); assert_eq!(a, Action::Pick("src/lib.rs".into()));
        apply(&mut s, Event::Ui(a)); assert_eq!(s.ui.tab, Tab::Tree);
        let f = frame(&s, 42, 0); assert_eq!(f.lines.iter().flatten().find(|g| g.text == "lib.rs").map(|g| (g.tone, g.bold)), Some((Tone::Pink, true)));
        // [ ⊟ ] folds everything.
        let (x, y) = at(&f, "⊟"); let a = click(&f, x, y).unwrap(); assert_eq!(a, Action::Fold);
        apply(&mut s, Event::Ui(a)); let l = render_lines(&s, 42);
        find(&l, "├── 📁 src (2 items)"); assert!(!l.iter().any(|x| x.contains("a.rs") || x.contains("⊟")));
        // Loading, error, very long trees, narrow widths.
        s.tree = None; find(&render_lines(&s, 42), "Escaneando…");
        s.tree = Some(Tree { error: Some("permission denied".into()), ..Tree::default() }); find(&render_lines(&s, 42), "permission denied");
        let many = (0..400).map(|i| TreeNode { name: format!("f{i:03}.txt"), rel: format!("f{i:03}.txt"), ..TreeNode::default() }).collect();
        s.tree = Some(Tree { project: "repo".into(), nodes: many, ..Tree::default() }); s.ui.tree_open.clear();
        let l = render_lines(&s, 42); find(&l, "f299.txt"); assert!(!l.iter().any(|x| x.contains("f300.txt"))); find(&l, "… 100 más");
        for w in [20u16, 30, 36, 42, 80] { for x in render_lines(&tree_sample(), w) { assert!(x.width() <= w as usize, "{w} {x:?}"); } }
    }
    fn roster_sample() -> State {
        let mut s = sample();
        let a = |n: &str, m: &str, t: &str| Agent { name: n.into(), model: m.into(), thinking: t.into() };
        s.roster = Some(Roster { model: "cpamc/gpt-6.1-sol".into(), thinking: "medium".into(), total: 6, done: 5, failed: 0, active: 1, running: vec![Running { text: "gentle-ai-worker · build".into(), elapsed: "3m".into() }], avg: "4m".into(), measured: 81,
            agents: vec![a("gentle-ai-explore", "cpamc/gpt-6.1-sol", "medium"), a("jd-judge-a", "x/claude-opus-high", "high"), a("my-helper", "y/long-model-name-v2", "xhigh"), a("review-risk", "", "")], ..Roster::default() });
        s
    }
    #[test]
    fn prof_host_falls_back_to_session_defaults_without_orchestrator() {
        let mut s=roster_sample();
        s.roster=Some(configured_roster(s.roster.take().unwrap(),Some(&serde_json::json!({
            "review-refuter":{"model":"canonical/profile-refuter"}
        })),"/gentle:profiles · global"));
        let prof=on(&s,Tab::Prof,60);
        let host=find(&prof,"🎯 host / orquestador");
        assert!(host.contains("gpt-6.1-sol (medium)"),"un perfil sin orchestrator conserva el host de la sesión: {host}");
        find(&prof,"profile-refuter");
    }
    #[test]
    fn canonical_prof_does_not_change_main_host_fallback() {
        let mut s=roster_sample(); s.session.model.clear(); s.session.thinking.clear();
        let before=on(&s,Tab::Main,60);
        s.roster=Some(configured_roster(s.roster.take().unwrap(),Some(&serde_json::json!({
            "orchestrator":{"model":"canonical/profile-host","thinking":"high"},
            "review-refuter":{"model":"canonical/profile-refuter"}
        })),"/gentle:profiles · global"));
        assert_eq!(on(&s,Tab::Main,60),before,"PROF must not rewrite STATUS host fallback");
        let prof=on(&s,Tab::Prof,60); find(&prof,"/gentle:profiles · global");
        find(&prof,"profile-host (high)"); find(&prof,"profile-refuter");
        assert!(!prof.iter().any(|l|l.contains("long-model-name-v2") || l.contains("claude-opus")));
        find(&prof,"5 ✓ · 1 en curso");
    }
    #[test]
    fn session_profile_marks_the_loaded_row_and_announces_itself() {
        let mut s = roster_sample();
        let mut r = s.roster.take().unwrap();
        r.profiles = profiles(Some(&serde_json::json!({
            "kind":"gentle-pi.agent_model_profiles","version":1,"active":"dos",
            "profiles":{"uno":{"orchestrator":{"model":"a/host-1"},"worker":{"model":"a/m2"}},
                "dos":{"orchestrator":"b/m1"},"raro ..":{"worker":"c/m1"}}})));
        r.pinned = Some("dos".into());
        s.roster = Some(r);
        // Channel up, no binding yet: no ◉ anywhere, the channel is not reported as down.
        s.profile_channel = Some(None);
        let l = on(&s, Tab::Prof, 48);
        let store=l.iter().filter(|l| l.contains("Perfiles")).count();
        let _=store;
        assert!(!l.iter().any(|l| l.contains("◉ ")), "sin binding no hay marca ◉: {l:#?}");
        assert!(!l.iter().any(|l| l.contains("canal no disponible")), "canal arriba sin binding no es un error");
        // A loaded session profile: its row carries ◉ (distinct from ● and ▣) and the
        // announcement names it.
        s.profile_channel = Some(Some(serde_json::json!({"name":"uno","models":{}})));
        let l = on(&s, Tab::Prof, 48);
        let row=find(&l, "uno");
        assert!(row.contains("◉"), "la fila del perfil cargado lleva ◉: {row}");
        // The store's own active and the repo pin keep their marks beside the session's ◉.
        find(&l, "● ▣ dos");
        assert!(l.iter().any(|l| l.contains("sesión:") && l.contains("uno")), "el anuncio nombra el perfil de sesión");
        // Channel down (extension not loaded): the card names it, never as a panic.
        s.profile_channel = None; s.profile_channel_down = true;
        let l = on(&s, Tab::Prof, 48);
        assert!(l.iter().any(|l| l.contains("canal no disponible")), "el canal caído se nombra");
    }
    #[test]
    fn session_profile_rewrites_profiles_card_with_its_models() {
        let mut s = roster_sample();
        // The loaded profile's model map replaces the global store rows, labeled by name.
        s.profile_channel = Some(Some(serde_json::json!({
            "name":"beta",
            "models":{"orchestrator":{"model":"p/beta-host","tier":"high"},"explore":{"model":"p/beta-explore"}}
        })));
        let l = on(&s, Tab::Prof, 60);
        assert!(l.iter().any(|l| l.contains("perfil de sesión") && l.contains("beta")), "la card nombra el perfil de sesión");
        find(&l, "beta-host (high)");
        find(&l, "explore"); find(&l, "beta-explore");
        // The global store's rows do not leak into the session view.
        assert!(!l.iter().any(|l| l.contains("long-model-name-v2")), "el store global no se mezcla con la sesión");
        // No orchestrator entry in the profile: the live host model stays.
        s.profile_channel = Some(Some(serde_json::json!({
            "name":"gamma",
            "models":{"explore":{"model":"p/gamma-explore"}}
        })));
        let l = on(&s, Tab::Prof, 60);
        let host_line=l.iter().find(|l| l.contains("host / orquestador")).unwrap();
        let host_block=&l[l.iter().position(|l| l.contains("host / orquestador")).unwrap()..].join("\n");
        assert!(host_block.contains("gpt-6.1-sol"), "sin orchestrator queda el host vivo: {host_block}");
        find(&l, "gamma-explore");
        // Channel down: the card says so and keeps the global view.
        s.profile_channel = None; s.profile_channel_down = true;
        let l = on(&s, Tab::Prof, 60);
        assert!(l.iter().any(|l| l.contains("canal de perfiles no disponible")));
        assert!(l.iter().any(|l| l.contains("gentle-ai-explore")), "vista global intacta con el canal caído");
    }
    #[test]
    fn failed_live_switch_is_visible_in_the_card() {
        // The routing shown is the profile's configuration; when the session could not adopt
        // it, the card says so instead of implying a switch that did not happen.
        let mut s = roster_sample();
        s.profile_channel = Some(Some(serde_json::json!({
            "name":"beta","live":"failed",
            "models":{"orchestrator":{"model":"p/beta-host"}}
        })));
        let l = on(&s, Tab::Prof, 60);
        assert!(l.iter().any(|l| l.contains("switch en vivo falló")), "el fallo del switch se muestra: {l:#?}");
        // A successful or absent outcome adds no noise.
        s.profile_channel = Some(Some(serde_json::json!({"name":"beta","live":"switched","models":{}})));
        let l = on(&s, Tab::Prof, 60);
        assert!(!l.iter().any(|l| l.contains("switch en vivo falló")));
    }
    #[test]
    fn changes_card_stages_and_only_commits_when_it_can() {
        let mut s = roster_sample();
        // Two files: one unstaged, one already staged; the secret one is offered no click.
        s.git.changes = parse_changes("M  staged.txt\0 M loose.txt\0?? .env\0");
        s.ui.tab = Tab::Git;
        let l = on(&s, Tab::Git, 48);
        assert!(l.iter().any(|l| l.contains("M  loose.txt") && l.contains("[stage]")), "sin stage lleva [stage]: {l:#?}");
        assert!(l.iter().any(|l| l.contains("M  staged.txt") && l.contains("[unstage]")), "staged lleva [unstage]");
        assert!(l.iter().any(|l| l.contains("?  .env") && l.contains("[rail]")), "un archivo sensible no se stagea desde el rail");
        // The unstaged row's click stages it; a staged row's click unstages it.
        let f = frame(&s, 48, 0);
        let (x, y) = at(&f, "loose.txt");
        assert_eq!(click(&f, x, y), Some(Action::Stage("loose.txt".into())));
        let (x, y) = at(&f, "staged.txt");
        assert_eq!(click(&f, x, y), Some(Action::Unstage("staged.txt".into())));
        // The sensitive row is inert: no action comes out of it.
        let (x, y) = at(&f, ".env");
        assert_eq!(click(&f, x, y), None, "la fila sensible no dispara nada");
        // act() re-checks the gate: a hand-built event must not stage a sensitive path.
        assert!(!apply(&mut s, Event::Ui(Action::Stage(".env".into()))));
        assert_eq!(s.ui.git_stage, None);
        // Commit is only offered with staged files and a message.
        s.ui.commit_message.clear();
        let l = on(&s, Tab::Git, 48);
        assert!(l.iter().any(|l| l.contains("[✓ commit]") && l.contains("sin mensaje")), "{l:#?}");
        s.ui.commit_message = "feat: algo".into();
        let f = frame(&s, 48, 0);
        let (x, y) = at(&f, "[✓ commit]");
        assert_eq!(click(&f, x, y), Some(Action::CommitNow));
        assert!(apply(&mut s, Event::Ui(Action::CommitNow)));
        assert!(s.ui.git_commit_now, "el clic queda como intención para main");
        // An empty message never sets the intent, even as a hand-built event.
        let mut empty = s.clone(); empty.ui.commit_message = "   ".into(); empty.ui.git_commit_now = false;
        assert!(!apply(&mut empty, Event::Ui(Action::CommitNow)));
        assert!(!empty.ui.git_commit_now);
        // The last write is reported verbatim, success or failure.
        s.git_notice = Some((false, "error: hook falló\nsegunda línea".into()));
        let l = on(&s, Tab::Git, 48);
        assert!(l.iter().any(|l| l.contains("✖ error: hook falló")), "{l:#?}");
        assert!(l.iter().any(|l| l.contains("segunda línea")));
        for w in [36u16, 42, 48] { for l in on(&s, Tab::Git, w) { assert!(l.width() <= usize::from(w), "{w} {l:?}"); } }
    }
    #[test]
    fn wrapped_channel_reply_still_names_the_session_profile() {
        // The worker used to emit the raw {ok, profile} reply, which blanked the
        // cards ("perfil de sesión:" with no name, every global row gone). Both
        // shapes must render the same name.
        let mut s = roster_sample();
        for shape in [serde_json::json!({"name":"uno","models":{}}),
                      serde_json::json!({"ok":true,"profile":{"name":"uno","models":{}}})] {
            s.profile_channel = Some(Some(shape));
            let l = on(&s, Tab::Prof, 60);
            let row = l.iter().find(|l| l.contains("perfil de sesión")).expect("row");
            assert!(row.contains("uno"), "la card nombra el perfil de sesión: {row}");
            assert!(!l.iter().any(|l| l.contains("explore")), "el store global no se mezcla");
        }
    }
    #[test]
    fn profiles_button_is_always_present_and_sends_the_command_intent() {
        // With no profiles at all the button still opens the screen: it is the tab's door to /gentle:profiles.
        let s = roster_sample();
        let l = on(&s, Tab::Prof, 48);
        let mut sp = s.clone(); sp.ui.tab = Tab::Prof;
        let f = frame(&sp, 48, 0); let (x, y) = at(&f, "/gentle:profiles");
        assert_eq!(click(&f, x, y), Some(Action::ProfilesCommand));
        let mut s = s;
        assert!(apply(&mut s, Event::Ui(Action::ProfilesCommand)));
        assert!(s.ui.open_profiles, "el clic queda como intención que main consume una vez");
        assert!(!s.ui.save, "nada se escribe por abrir la pantalla");
        // The row survives the narrow rail with the command readable.
        for width in [36u16, 42, 48] {
            let l = on(&s, Tab::Prof, width);
            assert!(l.iter().any(|l| l.contains("/gentle:profiles")), "el botón sigue visible a {width}");
            assert!(l.iter().all(|l| l.width() <= usize::from(width)));
        }
    }
    #[test]
    fn health_card_reports_stale_layers_roles_and_session() {
        // Nothing wrong: one row, no invented warnings. The sample carries a role with no model,
        // so the healthy case gives that role a model first.
        let mut s = roster_sample();
        if let Some(r) = s.roster.as_mut() { for a in r.agents.iter_mut() { if a.model.is_empty() { a.model="z/model".into(); } } }
        let healthy = on(&s, Tab::Prof, 60).join("\n");
        assert!(healthy.contains("✔ ruteo consistente"), "sin problemas lo dice en una fila: {healthy}");
        assert!(!healthy.contains("aviso"), "no se inventan avisos");
        // A pin layer pointing at a profile the store lost, and the role with no model back.
        if let Some(r) = s.roster.as_mut() {
            for a in r.agents.iter_mut() { if a.name == "review-risk" { a.model.clear(); } }
            r.profiles = profiles(Some(&serde_json::json!({"kind":"gentle-pi.agent_model_profiles","version":1,
                "profiles":{"uno":{"orchestrator":{"model":"a/host"}},"dos":{"worker":{"model":"b/m2"}}}})));
            r.pinned = Some("uno".into());
            r.pin_stale = vec![("clon".into(),"fantasma".into())];
        }
        let l = on(&s, Tab::Prof, 60);
        assert!(l.iter().any(|x| x.contains("Salud del ruteo") && x.contains("2 avisos")), "el título cuenta los avisos: {l:#?}");
        assert!(l.iter().any(|x| x.contains("pin clon: fantasma") && x.contains("no está en el store")), "el pin huérfano se nombra");
        assert!(l.iter().any(|x| x.contains("recreá fantasma")), "y la salida que de verdad lo arregla, no `publish`");
        assert!(l.iter().any(|x| x.contains("rol review-risk") && x.contains("sin modelo")), "el rol sin modelo se cuenta");
        // A session profile that left the store would otherwise render an empty map as healthy.
        s.profile_channel = Some(Some(serde_json::json!({"name":"borrado","models":{}})));
        let l = on(&s, Tab::Prof, 60);
        assert!(l.iter().any(|x| x.contains("sesión: borrado") && x.contains("no está en el store")), "{l:#?}");
        // The roles judged are the ones that resolve: with a session profile loaded, a role the
        // profile resolves is not warned about even when the global roster lacks it.
        s.profile_channel = Some(Some(serde_json::json!({"name":"uno","models":{"review-risk":{"model":"z/model"}}})));
        let l = on(&s, Tab::Prof, 60);
        assert!(!l.iter().any(|x| x.contains("rol review-risk")), "el perfil de sesión ya resuelve ese rol: {l:#?}");
        // And a role the loaded profile does NOT resolve is warned about, even if the global
        // roster has it.
        s.profile_channel = Some(Some(serde_json::json!({"name":"uno","models":{"gentle-ai-explore":{"model":""}}})));
        let l = on(&s, Tab::Prof, 60);
        assert!(l.iter().any(|x| x.contains("rol gentle-ai-explore") && x.contains("sin modelo")), "el perfil de sesión pierde ese rol: {l:#?}");
        // An unreadable store is not a clean routing: the card says it cannot judge.
        s.profile_channel = None;
        if let Some(r) = s.roster.as_mut() { r.store_missing = true; }
        let l = on(&s, Tab::Prof, 60);
        assert!(l.iter().any(|x| x.contains("Salud del ruteo") && x.contains("sin evaluar")), "el título dice que no se pudo evaluar: {l:#?}");
        assert!(l.iter().any(|x| x.contains("store de perfiles ilegible")), "y la fila dice por qué");
        assert!(!l.iter().any(|x| x.contains("ruteo consistente")), "un store ilegible nunca se reporta como sano");
        if let Some(r) = s.roster.as_mut() { r.store_missing = false; }
        // Narrow widths keep every row inside the rail.
        for w in [36u16, 42, 60] { for row in on(&s, Tab::Prof, w) { assert!(row.width() <= usize::from(w), "{w} {row:?}"); } }
    }
    #[test]
    fn profile_store_card_lists_profiles_and_picks_without_writing() {
        let mut s = roster_sample();
        let mut r = s.roster.take().unwrap();
        r.profiles = profiles(Some(&serde_json::json!({
            "kind":"gentle-pi.agent_model_profiles","version":1,"active":"dos",
            "profiles":{"uno":{"orchestrator":{"model":"a/host-1"},"worker":{"model":"a/m2"}},
                "dos":{"orchestrator":"b/m1"},"raro ..":{"worker":"c/m1"}}})));
        s.roster = Some(r); s.ui.tab = Tab::Prof;
        let l = on(&s, Tab::Prof, 48);
        assert!(find(&l, "● dos").contains("b/m1"), "el activo lleva su marca y el modelo del host");
        assert!(find(&l, "○ uno").contains("2 roles"), "la cuenta de roles es el dato distintivo");
        find(&l, "○ raro ..");
        // A click on a valid name sends the session-bind intent (`/profile`, a session-only
        // binding); nothing is written by the rail itself.
        let f = frame(&s, 48, 0); let (x, y) = at(&f, "○ uno");
        assert_eq!(click(&f, x, y), Some(Action::UseProfile("uno".into())));
        assert!(apply(&mut s, Event::Ui(Action::UseProfile("uno".into()))));
        assert_eq!(s.ui.use_profile.as_deref(), Some("uno"));
        assert!(!s.ui.save, "la publicación no toca el disco desde el rail");
        // The visual selection is still what an unsendable name falls back to.
        let f = frame(&s, 48, 0); let (x, y) = at(&f, "○ raro ..");
        assert_eq!(click(&f, x, y), Some(Action::PickProfile("raro ..".into())));
        assert!(apply(&mut s, Event::Ui(Action::PickProfile("raro ..".into()))));
        assert_eq!(s.ui.profile_pick.as_deref(), Some("raro .."));
        assert!(on(&s, Tab::Prof, 48).iter().any(|l| l.contains("▸ raro ..")), "la selección se ve");
        apply(&mut s, Event::Ui(Action::PickProfile("raro ..".into())));
        assert_eq!(s.ui.profile_pick, None);
    }
    #[test]
    fn a_profile_click_publishes_only_a_valid_name_and_marks_the_pinned_one() {
        let mut s = roster_sample();
        let mut r = s.roster.take().unwrap();
        r.profiles = profiles(Some(&serde_json::json!({
            "kind":"gentle-pi.agent_model_profiles","version":1,"active":"dos",
            "profiles":{"uno":{"worker":"m"},"dos":{"worker":"m"},"raro ..":{"worker":"m"}}})));
        r.pinned = Some("dos".into());
        s.roster = Some(r); s.ui.tab = Tab::Prof;
        // The pinned row carries ◆ (the store's active ● stays its own mark).
        let l = on(&s, Tab::Prof, 48);
        assert!(find(&l, "▣ dos").contains("●"), "el activo ● y el pineado ▣ conviven");
        // A pinned-but-unknown name is announced as stale, never as a live pin.
        // The announcement row names what subagent launches resolve.
        assert!(l.iter().any(|l| l.contains("subagentes") && l.contains("dos")), "el anuncio nombra el perfil: {l:#?}");
        // A valid name click sends the session-bind intent with that exact name.
        let f = frame(&s, 48, 0); let (x, y) = at(&f, "▣ dos");
        assert_eq!(click(&f, x, y), Some(Action::UseProfile("dos".into())));
        assert!(apply(&mut s, Event::Ui(Action::UseProfile("dos".into()))));
        assert_eq!(s.ui.use_profile.as_deref(), Some("dos"));
        assert!(!s.ui.save);
        // A weird-but-stored name never becomes a command argument: the click keeps the old
        // visual-selection behaviour and sends nothing.
        let f = frame(&s, 48, 0); let (x, y) = at(&f, "raro ..");
        assert_eq!(click(&f, x, y), Some(Action::PickProfile("raro ..".into())));
    }
    #[test]
    fn one_or_zero_profiles_keep_the_prof_tab_quiet() {
        for store in [serde_json::json!({"kind":"gentle-pi.agent_model_profiles","version":1}),
                      serde_json::json!({"kind":"gentle-pi.agent_model_profiles","version":1,"profiles":{"only":{"worker":"m"}}})] {
            let mut s = roster_sample();
            let mut r = s.roster.take().unwrap();
            r.profiles = profiles(Some(&store));
            s.roster = Some(r);
            let l = on(&s, Tab::Prof, 48);
            assert!(!l.iter().any(|l| l.contains("Perfiles")), "sin elección no hay card de store: {store}");
        }
    }
    #[test]
    fn profile_rows_survive_narrow_widths() {
        let mut s = roster_sample();
        let mut r = s.roster.take().unwrap();
        r.profiles = profiles(Some(&serde_json::json!({
            "kind":"gentle-pi.agent_model_profiles","version":1,"active":"largo",
            "profiles":{"largo":{"orchestrator":"provider/model-with-a-very-long-name-x"},"otro":{"worker":"m"}}})));
        s.roster = Some(r);
        for width in [36u16, 42, 48] {
            let l = on(&s, Tab::Prof, width);
            assert!(l.iter().all(|l| l.width() <= usize::from(width)), "nada se pasa de {width}");
            assert!(l.iter().any(|l| l.contains("largo")) && l.iter().any(|l| l.contains("otro")), "ambos nombres legibles a {width}");
        }
        // Narrow: the long host model takes its own row instead of clipping the name beside it.
        let l = on(&s, Tab::Prof, 36);
        assert!(!find(&l, "● largo").contains("provider/model"), "el modelo largo no va al lado del nombre");
    }
    #[test]
    fn prof_tab_lists_host_and_subagents() {
        let s = roster_sample(); let l = on(&s, Tab::Prof, 60);
        let host = find(&l, "🎯 host / orquestador"); assert!(host.contains("gpt-6.1-sol (medium)") && !host.contains("cpamc/"), "{host}");
        assert!(find(&l, "• gentle-ai-explore").ends_with("gpt-6.1-sol (medium) ║"));
        assert!(find(&l, "• jd-judge-a").contains("claude-opus (high)"), "-high suffix dropped like shortModelName");
        assert!(find(&l, "• review-risk").contains("sin modelo"), "un rol sin modelo lo dice, no inventa una asignación");
        let counters = find(&l, "📋 Tareas");
        assert!(counters.contains("5 ✓ · 1 en curso"), "{counters}");
        assert!(l.iter().any(|x| x.contains("▶ gentle-ai-worker · build") && x.contains("· 3m")), "la fila de la tarea viva dice hace cuánto corre");
        assert!(l.iter().any(|x| x.contains("⏱ 4m promedio · 81 medidas")), "y el promedio de las terminadas");
        // Failures are said out loud, and every running task is listed, not just the newest.
        let mut f = roster_sample();
        if let Some(r) = f.roster.as_mut() {
            r.failed = 2; r.active = 3;
            r.running = vec![
                Running { text: "a · uno".into(), elapsed: "2m".into() },
                Running { text: "b · dos".into(), elapsed: "1m".into() },
                Running { text: "c · tres".into(), elapsed: "9s".into() },
            ];
        }
        let l = on(&f, Tab::Prof, 60);
        let row = find(&l, "📋 Tareas");
        assert!(row.contains("5 ✓") && row.contains("2 ✗") && row.contains("3 en curso"), "{row}");
        assert!(l.iter().any(|x| x.contains("▶ a · uno") && x.contains("· 2m")) && l.iter().any(|x| x.contains("▶ c · tres")), "las tres vivas, con su tiempo");
        // Narrow: the counters would not fit beside the title, so they take a row of their own instead of
        // being clipped to "Task Man".
        let narrow = on(&f, Tab::Prof, 36);
        let i = narrow.iter().position(|x| x.contains("📋 Tareas")).unwrap();
        assert!(!narrow[i].contains("✓"), "el título queda solo: {}", narrow[i]);
        assert!(narrow[i + 1].contains("5 ✓") && narrow[i + 1].contains("2 ✗"), "y los contadores enteros: {}", narrow[i + 1]);
        // Family tones of the reference: ODD core cyan, Judgment Day gold, review mint, custom pink bullet + text.
        let mut p = s.clone(); p.ui.tab = Tab::Prof; let f = frame(&p, 60, 0); let tone = |needle: &str| f.lines.iter().flatten().find(|g| g.text == needle).map(|g| g.tone);
        assert_eq!((tone("gentle-ai-explore"), tone("jd-judge-a"), tone("review-risk"), tone("my-helper")), (Some(Tone::Cyan), Some(Tone::Gold), Some(Tone::Mint), Some(Tone::Text)));
        // Narrow: name and model do not fit together → the model moves to its own row, nothing is cut.
        let l = on(&s, Tab::Prof, 36); let i = l.iter().position(|x| x.contains("• my-helper")).unwrap();
        assert!(!l[i].contains("long-model") && l[i + 1].contains("long-model-name-v2 (xhigh)"), "{:#?}", &l[i..i + 2]);
        for w in [20u16, 36, 38, 42, 80] { for x in on(&s, Tab::Prof, w) { assert!(x.width() <= w as usize, "{w} {x:?}"); } }
        let mut e = sample(); e.roster = None; let l = on(&e, Tab::Prof, 42);
        find(&l, "⌁ Profiles & Clusters"); find(&l, "sin subagentes configurados"); assert!(!l.iter().any(|x| x.contains("Task Manager")));
    }
    #[test]
    fn mem_tab_engram_handoff_stack_tools_and_mcp() {
        let mut s = sample(); s.ui.tab = Tab::Mem;
        s.session.grep = 2; s.session.fetch = 1; s.session.search = 3; s.session.other = 4; s.session.tools = 14;
        s.engram.handoff = Some(Handoff { id: 214, kind: "session_summary".into(), at: Some(now_secs() - 7200),
            lines: vec!["## Goal".into(), "Ship PR #12 to main -> done".into(), "- merged #12 into develop".into()] });
        s.stack = vec![Tool { name: "pi", version: Some("1.5.0".into()) }, Tool { name: "gentle-ai", version: None }];
        let l = render_lines(&s, 42);
        assert!(find(&l, "🧠 Engram: repo").contains("[-]"));
        assert!(find(&l, "💻 Local (7437)").ends_with("● Online · 7 obs ║")); find(&l, "☁ Cloud");
        assert!(find(&l, "Estado:").contains("○ No sincronizado"));
        assert!(find(&l, "#214 session_summary").contains("hace 2h")); find(&l, "✦ Goal"); find(&l, "• merged #12 into develop");
        let f = frame(&s, 42, 0); let tone = |needle: &str| f.lines.iter().flatten().find(|g| g.text == needle).map(|g| g.tone);
        assert_eq!((tone("main"), tone("#12"), tone("➔"), tone("done")), (Some(Tone::Mint), Some(Tone::Gold), Some(Tone::Pink), Some(Tone::Cyan)));
        find(&l, "⌁ Stack · Ecosistema"); find(&l, "● pi v1.5.0"); find(&l, "○ gentle-ai · no verificada");
        // Tool pills wrap (3-space gap) instead of being cut; the call count sits in the title bar.
        let w38 = render_lines(&s, 38); assert!(find(&w38, "🔧 tools").contains("14 calls [-]"), "{w38:#?}");
        let pills: String = w38.iter().skip_while(|x| !x.contains("🔧 tools")).skip(1).take_while(|x| !x.contains('╚')).cloned().collect();
        for p in ["✎ 1 read", "✎ 2 write", ">_ 1 bash", "🧠 1 engram", "🔍 2 grep", "↓ 1 fetch", "◆ 3 search", "⚙ 4 other"] { assert!(pills.contains(p), "{p} in {pills}"); }
        assert!(find(&l, "⌁ MCP · 1").contains("[-]"));
        // Enrolled → Sync row with the last sync; syncing phase; daemon down and DB error.
        s.engram.enrolled = true; s.engram.sync = Some(("idle".into(), Some(now_secs() - 120)));
        assert!(find(&render_lines(&s, 42), "Sync:").contains("● Enrolado (hace 2m)"));
        s.engram.sync = Some(("syncing".into(), None)); assert!(find(&render_lines(&s, 42), "Sync:").contains("⟳ Sincronizando…"));
        s.engram = Engram { project: "repo".into(), error: Some("unable to open".into()), ..Engram::default() }; s.stack.clear(); s.session = Session::default();
        let l = render_lines(&s, 42);
        assert!(find(&l, "Local (7437)").contains("○ Caído · — obs")); find(&l, "unable to open");
        find(&l, "Sin resumen de sesión previo"); find(&l, "Detectando herramientas…"); find(&l, "sin llamadas todavía");
        for w in [20u16, 36, 38, 42, 80] { for x in render_lines(&s, w) { assert!(x.width() <= w as usize, "{w} {x:?}"); } }
    }
    #[test]
    fn long_handoff_is_bounded() {
        let mut s = sample(); s.ui.tab = Tab::Mem;
        s.engram.handoff = Some(Handoff { id: 1, kind: "session_summary".into(), at: None, lines: (0..40).map(|i| format!("line {i}")).collect() });
        let l = render_lines(&s, 42); find(&l, "#1 session_summary").contains("nunca"); find(&l, "line 23"); assert!(!l.iter().any(|x| x.contains("line 24")));
        find(&l, "… 16 líneas más");
    }
    #[test]
    fn todos_card_mirrors_the_session_checklist() {
        let mut s = sample(); let l = render_lines(&s, 42);
        assert!(find(&l, "⌁ Todos · 0 of 0").starts_with(" ╔═ ")); find(&l, "sin tareas en esta sesión");
        let t = |title: &str, status, note: Option<&str>| Todo { title: title.into(), status, note: note.map(Into::into) };
        s.session.todos = vec![t("Escribir el parser", TodoStatus::Done, Some("hidden when done")),
            t("Conectar el mouse y las teclas del panel lateral de monitoreo", TodoStatus::InProgress, Some("probando clicks reales en herdr con SGR")),
            t("Docs", TodoStatus::Pending, None)];
        let l = render_lines(&s, 42); let i = l.iter().position(|l| l.contains("⌁ Todos · 1 of 3")).unwrap();
        let body: Vec<&str> = l[i + 1..].iter().take_while(|l| !l.contains('╚')).map(|l| l[" ║ ".len()..l.len() - " ║".len()].trim_end()).collect();
        assert_eq!(body, ["✓ Escribir el parser", "◉ Conectar el mouse y las teclas del", "  panel lateral de monitoreo",
            "  ↳ probando clicks reales en herdr", "    con SGR", "○ Docs"]);
        assert!(!l.iter().any(|l| l.contains("hidden when done")));
        // Tones: done dim, in progress pinkBright with a dim note, glyphs mint / gold / dim.
        let f = frame(&s, 42, 0); let rows: Vec<&Vec<Seg>> = f.lines.iter().filter(|l| plain(l).contains("Escribir") || plain(l).contains("Conectar") || plain(l).contains("↳") || plain(l).contains("○ Docs")).collect();
        let tone = |r: &Vec<Seg>, txt: &str| r.iter().find(|g| g.text.contains(txt)).map(|g| g.tone);
        assert_eq!((tone(rows[0], "✓"), tone(rows[0], "Escribir")), (Some(Tone::Mint), Some(Tone::Dim)));
        assert_eq!((tone(rows[1], "◉"), tone(rows[1], "Conectar")), (Some(Tone::Gold), Some(Tone::PinkBright)));
        assert_eq!(tone(rows[2], "↳"), Some(Tone::Dim)); assert_eq!((tone(rows[3], "○"), tone(rows[3], "Docs")), (Some(Tone::Dim), Some(Tone::Text)));
        // A word longer than the row is cut, never overflowing.
        s.session.todos = vec![t(&"x".repeat(80), TodoStatus::Pending, None)];
        for w in [20u16, 42, 80] { for l in render_lines(&s, w) { assert!(l.width() <= w as usize); } }
    }
    fn git_sample() -> State {
        let mut s = sample(); s.ui.tab = Tab::Git;
        s.git.graph = vec!["* abc1234 (HEAD -> feat/x, origin/feat/x) first commit".into(), "|\\".into(), "| * 9f8e7d6 side".into(), "|/".into(), "* 1234567 base".into()];
        let f = |path: &str, kind, added, deleted| FileChange { path: path.into(), kind, added, deleted };
        s.git.files = vec![f("src/a.rs", FileKind::Modified, 3, 1), f("new.txt", FileKind::Untracked, 0, 0), f("gs3/hud/src/a_very_long_file_name_here.rs", FileKind::Added, 10, 0)];
        s.git.counts = Counts { staged: 0, modified: 1, untracked: 1, conflicts: 0 }; s.git.added = 13; s.git.deleted = 1; s.git.dirty = Some(3);
        let b = |name: &str, current, remote, ahead, behind, gone, date: &str| Branch { name: name.into(), current, remote, ahead, behind, gone, date: date.into() };
        s.git.branches = vec![b("feat/x", true, false, 1, 0, false, "5m"), b("main", false, false, 2, 1, false, "3d"), b("gone", false, false, 0, 0, true, "2w"), b("origin/old", false, true, 0, 0, false, "1y")];
        s
    }
    #[test]
    fn git_graph_card_with_view_toggle() {
        let mut s = git_sample(); let l = render_lines(&s, 42);
        let t = find(&l, "⎇ git graph"); assert!(t.contains("[ Grafo | Ramas ] [-] ═╗") && t.width() == 41, "{t:?}");
        assert!(find(&l, "feat/x ").contains("● 1 mod · ?1 untracked"), "branch row with status badges");
        let f = frame(&s, 80, 0); let row = f.lines.iter().find(|l| plain(l).contains("abc1234")).unwrap();
        let tone = |txt: &str| row.iter().find(|g| g.text.contains(txt)).map(|g| g.tone);
        assert_eq!([tone("*"), tone("abc1234"), tone("HEAD -> feat/x"), tone("origin/feat/x"), tone("first commit"), tone("(")],
            [Some(Tone::Pink), Some(Tone::Yellow), Some(Tone::Mint), Some(Tone::Secondary), Some(Tone::Text), Some(Tone::Muted)]);
        assert_eq!(f.lines.iter().find(|l| plain(l).contains("|/")).unwrap().iter().find(|g| g.text.contains('|')).unwrap().tone, Tone::Blue);
        find(&l, "✎ 1 file · +13 −1"); let f = frame(&s, 42, 0);
        // Click "Ramas": the branches view.
        let (x, y) = at(&f, "Ramas"); let a = click(&f, x + 1, y).unwrap(); assert_eq!(a, Action::GitView(GitView::Branches));
        apply(&mut s, Event::Ui(a)); let l = render_lines(&s, 42);
        find(&l, "● 4 ramas en repo"); assert!(find(&on(&s, Tab::Git, 60), "● 4 ramas en repo").ends_with("● 1 mod · ?1 untracked ║"), "badge when it fits");
        assert!(find(&l, "⎇ feat/x").ends_with("actual ║")); assert!(find(&l, "● main").ends_with("↑2 ↓1 ║"));
        assert!(find(&l, "● gone").ends_with("gone ║")); assert!(find(&l, "● rem/old").ends_with("1y ║"));
        assert!(!l.iter().any(|l| l.contains("abc1234")));
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "Grafo"); apply(&mut s, Event::Ui(click(&f, x + 1, y).unwrap())); assert_eq!(s.ui.git_view, GitView::Graph);
        // Narrow card: compact `[ G | R ]`; collapsed: no toggle.
        assert!(find(&render_lines(&s, 38), "⎇ git graph").contains("[ G | R ] [-] ═╗"));
        s.ui.config.collapsed = vec!["git".into()]; assert!(find(&render_lines(&s, 42), "⎇ git graph").ends_with("═ [+] ═╗"));
        s.git.files.clear(); s.git.counts = Counts::default(); s.git.dirty = Some(0); s.ui.config.collapsed.clear();
        assert!(find(&render_lines(&s, 42), "feat/x ").contains("✔ clean"));
    }
    #[test]
    fn commit_message_editor_confirms_and_cancels() {
        let mut s=git_sample();
        act(&mut s.ui,Action::EditCommitMessage);
        key(&mut s,Key::Char('h')); key(&mut s,Key::Char('i')); key(&mut s,Key::Back); key(&mut s,Key::Enter);
        assert_eq!(s.ui.commit_message,"h");
        act(&mut s.ui,Action::EditCommitMessage); key(&mut s,Key::Char('x')); key(&mut s,Key::Esc);
        assert_eq!(s.ui.commit_message,"h");
    }
    #[test]
    fn cambios_shows_nonempty_sections_and_counts_at_narrow_widths() {
        let mut s=git_sample();
        s.git.changes=parse_changes("M  staged.txt\0MM both.txt\0?? new.txt\0");
        s.git.changes.staged[0].numstat=Some((3,1));
        for width in [42,36] {
            let lines=on(&s,Tab::Git,width);
            let staged=lines.iter().position(|l|l.contains("staged")).expect("staged section");
            let unstaged=lines.iter().position(|l|l.contains("sin stage")).expect("unstaged section");
            assert!(staged<unstaged);
            assert!(find(&lines,"staged.txt").contains("+3 −1"));
            assert_eq!(lines.iter().filter(|l|l.contains("both.txt")).count(),2);
            assert!(lines.iter().all(|l|l.width()<=width as usize));
        }
        s.git.changes=Changes::default();
        let lines=on(&s,Tab::Git,36);
        assert!(lines.iter().any(|l|l.contains("faltan archivos staged")));
        assert!(!lines.iter().any(|l|l.contains("sin stage")));
    }
    #[test]
    fn working_tree_card_lists_changes() {
        let mut s = git_sample(); let l = render_lines(&s, 42);
        assert!(find(&l, "📁 Working Tree").starts_with(" ╔═ "));
        assert!(find(&l, "● 3 archivos modificados").contains("+13  −1"));
        assert!(find(&l, "M  src/a.rs").ends_with("[diff] +3 −1 ║")); assert!(find(&l, "?  new.txt").ends_with("[diff] ║"));
        assert!(find(&l, "A  …/a_very_long_file_").ends_with("[diff] +10 ║"), "the right side wins; the directory shrinks first");
        let f = frame(&s, 42, 0); let row = f.lines.iter().find(|l| plain(l).contains("src/a.rs")).unwrap();
        let tone = |txt: &str| row.iter().find(|g| g.text == txt).map(|g| g.tone);
        assert_eq!([tone("M"), tone("src/"), tone("a.rs"), tone("diff"), tone("+3"), tone("−1")], [Some(Tone::Yellow), Some(Tone::Dim), Some(Tone::Cyan), Some(Tone::Pink), Some(Tone::Mint), Some(Tone::Coral)]);
        assert!(find(&l, "click: ver diff").starts_with(" ║ "));
        s.git.files.clear(); find(&render_lines(&s, 42), "✔ Clean · Sin cambios pendientes");
        for w in [20u16, 30, 38, 42, 80] { for l in render_lines(&git_sample(), w) { assert!(l.width() <= w as usize, "{w} {l:?}"); } }
    }
    #[test]
    fn clicking_a_file_previews_its_diff_inline() {
        let mut s = git_sample(); let f = frame(&s, 42, 0);
        let (x, y) = at(&f, "src/a.rs"); let a = click(&f, x, y).unwrap(); assert_eq!(a, Action::File("src/a.rs".into()));
        apply(&mut s, Event::Ui(a)); assert_eq!(s.ui.git_file.as_deref(), Some("src/a.rs"));
        find(&render_lines(&s, 42), "cargando diff…");
        let diff = ["diff --git a/src/a.rs b/src/a.rs", "index 1..2 100644", "--- a/src/a.rs", "+++ b/src/a.rs", "@@ -1,2 +1,2 @@", " keep\tit", "-old", "+new"];
        apply(&mut s, Event::Detail(Detail { file: Some("src/a.rs".into()), diff: diff.iter().map(|l| l.to_string()).collect(), ..Detail::default() }));
        let l = render_lines(&s, 42); let i = l.iter().position(|l| l.contains("M  src/a.rs")).unwrap();
        let rows: Vec<&str> = l[i + 1..i + 5].iter().map(|l| l[" ║ ".len()..l.len() - " ║".len()].trim_end()).collect();
        assert_eq!(rows, ["  @@ -1,2 +1,2 @@", "   keep    it", "  -old", "  +new"]); assert!(l[i + 5].contains("?  new.txt"));
        let f = frame(&s, 42, 0); let tone = |txt: &str| f.lines.iter().flatten().find(|g| g.text.contains(txt)).map(|g| g.tone);
        assert_eq!([tone("@@ -1"), tone("-old"), tone("+new")], [Some(Tone::Blue), Some(Tone::Coral), Some(Tone::Mint)]);
        // Long diffs are capped; a second click closes the preview.
        s.detail.diff = (0..200).map(|i| format!("+line {i}")).collect(); let l = render_lines(&s, 42);
        find(&l, "… 120 líneas más"); assert!(!l.iter().any(|l| l.contains("+line 80")));
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "src/a.rs"); apply(&mut s, Event::Ui(click(&f, x, y).unwrap()));
        assert_eq!(s.ui.git_file, None); assert!(!render_lines(&s, 42).iter().any(|l| l.contains("+line")));
        // Possible secrets are never previewed; a file that left the working tree drops its selection.
        s.ui.git_file = Some(".env.local".into()); s.git.files.push(FileChange { path: ".env.local".into(), kind: FileKind::Untracked, ..FileChange::default() });
        find(&render_lines(&s, 42), "contenido oculto · posible secreto");
        let mut g = s.git.clone(); g.files.retain(|f| f.path != ".env.local"); apply(&mut s, Event::Git(g)); assert_eq!(s.ui.git_file, None);
    }
    #[test]
    fn clicking_a_branch_inspects_its_diff() {
        let mut s = git_sample(); s.ui.git_view = GitView::Branches;
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "⎇ feat/x"); assert_eq!(click(&f, x + 3, y), None, "the current branch is not a diff target");
        let (x, y) = at(&f, "● main"); let a = click(&f, x + 3, y).unwrap(); assert_eq!(a, Action::Branch(Some("main".into())));
        apply(&mut s, Event::Ui(a)); let l = render_lines(&s, 42);
        assert!(find(&l, "▶ main").ends_with("diff ║")); assert!(find(&l, "📁 Diff · main").contains("[ ✕ ] [-] ═╗")); find(&l, "cargando…");
        let fc = FileChange { path: "src/b.rs".into(), kind: FileKind::Added, added: 4, deleted: 0 };
        apply(&mut s, Event::Detail(Detail { branch: Some("main".into()), files: vec![fc], added: 4, ..Detail::default() }));
        let l = render_lines(&s, 42); assert!(find(&l, "● 1 archivo").contains("+4  −0")); assert!(find(&l, "A  src/b.rs").ends_with("[diff] +4 ║"));
        assert!(!l.iter().any(|l| l.contains("src/a.rs")), "the branch diff replaces the working tree");
        // Clicking a file of the branch diff selects it; ✕ closes the inspector and its file.
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "src/b.rs"); apply(&mut s, Event::Ui(click(&f, x, y).unwrap())); assert_eq!(s.ui.git_file.as_deref(), Some("src/b.rs"));
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "✕"); assert_eq!(click(&f, x, y), Some(Action::Branch(None)));
        apply(&mut s, Event::Ui(Action::Branch(None))); assert_eq!((s.ui.git_branch.clone(), s.ui.git_file.clone()), (None, None));
        find(&render_lines(&s, 42), "📁 Working Tree");
        apply(&mut s, Event::Ui(Action::Branch(Some("main".into())))); apply(&mut s, Event::Ui(Action::Branch(Some("main".into())))); assert_eq!(s.ui.git_branch, None, "same branch toggles");
    }

    #[test]
    fn a_click_on_a_commit_row_opens_the_files_it_touched() {
        let mut s = sample(); s.ui.tab = Tab::Git; s.ui.git_view = GitView::Graph;
        s.git.graph = vec!["* a1b2c3d (HEAD -> main) top".into()];
        let f = frame(&s, 60, 0);
        let (x, y) = at(&f, "top");
        assert_eq!(click(&f, x, y), Some(Action::Commit("a1b2c3d".into())), "the row names the commit it draws");
        // Opening a commit clears the file and the branch: they answer different questions.
        s.ui.git_file = Some("src/a.rs".into()); s.ui.git_branch = Some("dev".into());
        apply(&mut s, Event::Ui(Action::Commit("a1b2c3d".into())));
        assert_eq!((s.ui.git_commit.clone(), s.ui.git_file.clone(), s.ui.git_branch.clone()), (Some("a1b2c3d".into()), None, None));
        // The same commit closes it, and anything that is not what git printed never opens one.
        apply(&mut s, Event::Ui(Action::Commit("a1b2c3d".into())));
        assert_eq!(s.ui.git_commit, None, "same commit toggles");
        apply(&mut s, Event::Ui(Action::Commit("--upload-pack=x".into())));
        assert_eq!(s.ui.git_commit, None, "a line cannot smuggle an option into git");
        // The card lists what the commit touched, and a file in that list opens its diff.
        s.detail.commit = Some("a1b2c3d".into());
        s.detail.files = vec![FileChange { path: "src/a.rs".into(), kind: FileKind::Modified, added: 2, deleted: 1 }];
        apply(&mut s, Event::Ui(Action::Commit("a1b2c3d".into())));
        let f = frame(&s, 60, 0);
        let (x, y) = at(&f, "src/a.rs");
        assert_eq!(click(&f, x, y), Some(Action::File("src/a.rs".into())), "the commit's own file list is clickable");
        // The window gets plain text: the hash, one line per file, then the diff of the open file.
        let text = panel_text(&s.detail, "a1b2c3d");
        let plain = |t: &str| t.split("\u{1b}[").map(|p| p.splitn(2, 'm').nth(1).unwrap_or("")).collect::<String>();
        let shown = plain(&text);
        assert!(shown.starts_with("◆ commit a1b2c3d"), "{shown}");
        assert!(shown.contains("M src/a.rs"), "the file list is in the window too: {shown}");
        assert!(shown.contains("+2") && shown.contains("-1"), "with its counts: {shown}");
        assert!(text.contains("\u{1b}[38;2;"), "and it carries colour, or the window cannot show a diff");
        // The commit's own words arrive above the files: who, when, and the whole message.
        s.detail.author = "Ada Lovelace".into(); s.detail.date = "2 days ago".into();
        s.detail.message = "arregla el panel del commit\n\nSegundo párrafo del cuerpo.\n".into();
        let with_message = plain(&panel_text(&s.detail, "a1b2c3d"));
        assert!(with_message.contains("Ada Lovelace · 2 days ago"), "quién y cuándo: {with_message}");
        assert!(with_message.contains("arregla el panel del commit") && with_message.contains("Segundo párrafo del cuerpo."), "el mensaje completo: {with_message}");
        assert!(with_message.contains("M src/a.rs") && with_message.contains("[diff]"), "y los archivos siguen igual: {with_message}");
        assert_eq!(wrap_words("uno dos tres", 7), ["uno dos", "tres"], "se envuelve por palabras");
        assert_eq!(wrap_words(&"x".repeat(25), 10), ["x".repeat(10), "x".repeat(10), "x".repeat(5)], "un token más ancho que la columna se corta");
        let empty = plain(&panel_text(&Detail::default(), "a1b2c3d"));
        assert!(empty.contains("sin archivos"), "an empty commit says so: {empty}");
        s.detail.files.clear(); s.ui.git_branch = Some("main".into()); s.detail.branch = Some("main".into()); find(&render_lines(&s, 42), "✔ Sin diferencias con HEAD");
    }
    #[test]
    fn the_window_shows_the_picked_file_and_what_it_cut() {
        let plain = |t: &str| t.split("\u{1b}[").map(|p| p.splitn(2, 'm').nth(1).unwrap_or("")).collect::<String>();
        let text = preview_text(&Preview::Text { path: "src/main.rs".into(), body: "fn main() {\n\tok();\n}\n".into(), cut: false });
        let shown = plain(&text);
        assert!(shown.starts_with("🗒 archivo [ ✕ ] [-]"), "{shown}");
        assert!(shown.contains("src/main.rs") && shown.contains("fn main() {"), "{shown}");
        assert!(shown.contains("    ok();"), "a tab keeps the columns it was written with: {shown}");
        assert!(!shown.contains("sólo los primeros"), "nothing was cut: {shown}");
        assert!(text.contains("\u{1b}[38;2;"), "the window needs the rail's own colour, or a file reads flat");
        let cut = plain(&preview_text(&Preview::Text { path: "big.md".into(), body: "x".into(), cut: true }));
        assert!(cut.contains("sólo los primeros 256 KiB"), "{cut}");
        let refused = plain(&preview_text(&Preview::Refused { path: "secrets.yml".into(), note: "archivo sensible (no se muestra)".into() }));
        assert!(refused.contains("secrets.yml") && refused.contains("no se muestra"), "the window says why: {refused}");
    }
    #[test]
    fn one_window_serves_the_commit_and_the_picked_file() {
        let mut s = sample();
        assert!(panel_show(&s).is_none(), "MAIN opens nothing");
        s.ui.tab = Tab::Git; s.ui.git_commit = Some("a1b2c3d".into()); s.detail.commit = Some("a1b2c3d".into());
        assert!(matches!(panel_show(&s), Some((_, _, PanelCmd::Show { .. }))), "a commit has a window");
        s.detail.commit = None;
        assert!(panel_show(&s).is_none(), "no answer for that hash, no window");
        // An image is handed over as a path, so the window draws it and the rail never reads its bytes.
        s.ui.tab = Tab::Tree; s.ui.tree_pick = Some("assets/logo.png".into());
        s.preview = Some(Preview::Image { path: "assets/logo.png".into(), file: "/repo/assets/logo.png".into() });
        assert!(matches!(panel_show(&s), Some((ref k, _, PanelCmd::Image { .. })) if k == "file:assets/logo.png"));
        // An answer for a file that is no longer the pick shows nothing: no window under the wrong name.
        s.ui.tree_pick = Some("src/main.rs".into());
        assert!(panel_show(&s).is_none());
        s.preview = Some(Preview::Text { path: "src/main.rs".into(), body: "fn main() {}\n".into(), cut: false });
        let Some((key, _, PanelCmd::Show { text, .. })) = panel_show(&s) else { panic!("no window for a text file") };
        assert_eq!(key, "file:src/main.rs");
        assert!(text.contains("src/main.rs") && text.contains("fn main() {}"));
        // Markdown goes as source and a separate mode: the window renders it, the rail does not flatten it.
        s.preview = Some(Preview::Markdown { path: "docs/guia.md".into(), body: "# Título\n".into(), cut: true });
        s.ui.tree_pick = Some("docs/guia.md".into());
        let Some((key, _, PanelCmd::Markdown { text, cut, .. })) = panel_show(&s) else { panic!("no window for markdown") };
        assert_eq!((key.as_str(), text.as_str(), cut), ("file:docs/guia.md", "# Título\n", true));
        // Source code goes to the window already coloured, down the same text path the diff uses.
        s.preview = Some(Preview::Code { path: "src/main.rs".into(), body: "fn main() {}\n".into(), cut: false, lang: "Rust".into() });
        s.ui.tree_pick = Some("src/main.rs".into());
        let Some((key, _, PanelCmd::Show { text, .. })) = panel_show(&s) else { panic!("no window for code") };
        assert_eq!(key, "file:src/main.rs");
        assert!(text.contains("◆ Rust") && text.contains("src/main.rs"), "el encabezado dice el lenguaje: {text}");
        assert!(text.contains("\u{1b}[38;2;255;210;63m"), "la palabra clave llega pintada: {text}");
        // Leaving TREE closes it.
        s.ui.tab = Tab::Main;
        assert!(panel_show(&s).is_none());
    }
    #[test]
    fn the_graph_offers_one_more_page_and_stops_when_the_repo_ends() {
        let mut s = sample();
        s.ui.tab = Tab::Git;
        // A page that filled up: thirty asked, thirty read, so there may be more behind them.
        s.git.graph = (0..30).map(|i| format!("* abc{i:04} commit {i}")).collect();
        s.git.commits = (0..30).map(|i| format!("abc{i:04} commit {i}")).collect();
        s.git.log_limit = 30;
        let f = frame(&s, 60, 0);
        let (x, y) = at(&f, "cargar 30 más");
        let a = click(&f, x, y).expect("la fila de la página es clicable");
        assert_eq!(a, Action::MoreCommits(60), "la página siguiente duplica el pedido");
        assert!(apply(&mut s, Event::Ui(a)));
        assert_eq!(s.ui.git_more, Some(60), "y el pedido sale al worker");
        // Fewer commits than the page: the repository ended, so there is nothing more to ask for.
        s.git.commits.truncate(3);
        assert!(!render_lines(&s, 60).iter().any(|l| l.contains("cargar")), "sin más commits no hay fila");
    }

    #[test]
    fn the_graph_shows_a_window_of_25_and_the_wheel_walks_back() {
        let mut s = sample();
        s.ui.tab = Tab::Git;
        s.git.graph = (0..60).map(|i| format!("* abc{i:04} commit {i}")).collect();
        s.git.commits = (0..60).map(|i| format!("abc{i:04} commit {i}")).collect();
        s.git.log_limit = 60;
        let f = frame(&s, 60, 60);
        let l = render_lines(&s, 60);
        assert!(l.iter().any(|x| x.contains("commit 24")), "la ventana llega hasta la 25");
        assert!(!l.iter().any(|x| x.contains("commit 40")), "y no dibuja las 60");
        assert!(l.iter().any(|x| x.contains("↕ 1-25/60")), "una fila dice qué tramo se ve");
        // The wheel over the card walks the window, not the body of the page.
        let (_, y) = at(&f, "commit 3");
        assert_eq!(wheel(&f, y, true), Some(Action::ScrollGraph(3)));
        assert!(apply(&mut s, Event::Ui(Action::ScrollGraph(3))));
        assert_eq!(s.ui.git_scroll, 3);
        assert!(render_lines(&s, 60).iter().any(|x| x.contains("4-28/60")), "el tramo se sigue");
    }

    #[test]
    fn clicking_tabs_and_wheel_on_the_bar() {
        let mut s = sample(); let f = frame(&s, 60, 40);
        let (x, y) = at(&f, "MEM"); assert_eq!(y, f.tab_row);
        let a = click(&f, x + 1, y).expect("tab hit"); assert_eq!(a, Action::Tab(Tab::Mem));
        assert!(apply(&mut s, Event::Ui(a))); assert_eq!(s.ui.tab, Tab::Mem);
        let l = render_lines(&s, 60); assert!(l[4].contains("[ MEM ]")); find(&l, "🧠 Engram: repo");
        // The gear tab opens CONFIG, also at the 37-col pane width.
        let f = frame(&s, 37, 40); let bar = plain(&f.lines[f.tab_row]); let x = bar[..bar.find('⛭').unwrap()].width();
        assert_eq!(click(&f, x, f.tab_row), Some(Action::Tab(Tab::Config)));
        assert_eq!(click(&f, 0, 0), None);
        let f = frame(&s, 60, 40);
        apply(&mut s, Event::Ui(wheel(&f, f.tab_row, true).unwrap())); assert_eq!(s.ui.tab, Tab::Tree);
        apply(&mut s, Event::Ui(Action::Cycle(-1))); apply(&mut s, Event::Ui(Action::Cycle(-1))); assert_eq!(s.ui.tab, Tab::Prof);
        apply(&mut s, Event::Ui(Action::Tab(Tab::Main))); apply(&mut s, Event::Ui(Action::Cycle(-1))); assert_eq!(s.ui.tab, Tab::Config, "wraps");
        for (c, t) in [('2', Tab::Git), ('5', Tab::Tree), ('6', Tab::Media), ('7', Tab::Config), ('1', Tab::Main)] { apply(&mut s, Event::Key(Key::Char(c))); assert_eq!(s.ui.tab, t); }
        apply(&mut s, Event::Key(Key::Char('r'))); assert!(s.refresh); apply(&mut s, Event::Key(Key::Char('q'))); assert!(s.quit);
    }
    #[test]
    fn dash_collapses_and_restores_a_card() {
        let mut s = sample(); let f = frame(&s, 42, 0);
        let (x, y) = at(&f, "[-]"); let a = click(&f, x + 1, y).unwrap(); assert_eq!(a, Action::Collapse("status".into()));
        apply(&mut s, Event::Ui(a)); assert!(s.ui.save);
        let l = render_lines(&s, 42); assert!(find(&l, "⌁ Status").ends_with("[+] ═╗")); assert!(!l.iter().any(|l| l.contains("Project")));
        let i = l.iter().position(|l| l.contains("⌁ Status")).unwrap(); assert!(l[i + 1].trim_start().starts_with('╚'));
        find(&l, "⌁ Context");
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "[+]"); apply(&mut s, Event::Ui(click(&f, x + 1, y).unwrap()));
        find(&render_lines(&s, 42), "Project");
        assert_eq!(s.ui.config.collapsed, Vec::<String>::new());
    }
    #[test]
    fn body_scrolls_with_the_wheel_and_clamps() {
        let mut s = sample(); s.ui.tab = Tab::Config;
        let full = frame(&s, 42, 0).lines.len(); let f = frame(&s, 42, 20);
        assert_eq!(f.lines.len(), 20); assert_eq!(f.max_scroll, full - 20); assert!(plain(&f.lines[5]).ends_with(" ↓"));
        let mut n = 0; while let Some(a) = wheel(&frame(&s, 42, 20), 15, true) { apply(&mut s, Event::Ui(a)); n += 1; assert!(n < 50); }
        let f = frame(&s, 42, 20); assert_eq!(f.scroll, f.max_scroll); assert!(plain(&f.lines[5]).ends_with(" ↑"));
        assert!(plain(f.lines.last().unwrap()).trim_start().starts_with('╚'));
        // Hits follow the scrolled rows.
        let (x, y) = at(&f, "salir"); assert!(y < 20); let _ = x;
        apply(&mut s, Event::Ui(wheel(&f, 15, false).unwrap())); assert_eq!(s.ui.scroll[Tab::Config.index()], f.max_scroll - 3);
        // Other tabs keep their own offset.
        apply(&mut s, Event::Key(Key::Char('1'))); assert_eq!(frame(&s, 42, 20).scroll, 0);
    }
    #[test]
    fn config_hides_tabs_and_sets_the_name() {
        let mut s = sample(); apply(&mut s, Event::Key(Key::Char('7')));
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "[✓] TREE"); let a = click(&f, x, y).unwrap();
        assert_eq!(a, Action::Hide(Tab::Tree)); apply(&mut s, Event::Ui(a)); assert!(s.ui.save);
        let l = render_lines(&s, 42); assert!(!l[4].contains("TREE")); assert!(find(&l, "[ ] TREE").contains("oculta"));
        apply(&mut s, Event::Key(Key::Char('5'))); assert_eq!(s.ui.tab, Tab::Config, "hidden tab keys are ignored");
        assert!(find(&l, "[✓] ⛭ CONFIG").contains("fija"));
        let f = frame(&s, 42, 0); let (x, y) = at(&f, "⛭ CONFIG"); assert_eq!(click(&f, x, y + 0).filter(|a| matches!(a, Action::Hide(_))), None);
        // Name: click "editar", type, backspace, Enter; `q` is text while editing.
        s.ui.save = false; let f = frame(&s, 42, 0); let (x, y) = at(&f, "✎ editar"); apply(&mut s, Event::Ui(click(&f, x, y).unwrap()));
        for k in "Ramonsq".chars().map(Key::Char).chain([Key::Back, Key::Char('D'), Key::Char('e'), Key::Char('v')]) { apply(&mut s, Event::Key(k)); }
        assert!(!s.quit); find(&render_lines(&s, 42), "> RamonsDev▏");
        apply(&mut s, Event::Key(Key::Enter)); assert_eq!(s.ui.config.name, "RamonsDev"); assert!(s.ui.save);
        assert!(render_lines(&s, 42)[1].contains("⌁ RamonsDev ⌁"));
        // Esc cancels; an empty name restores the placeholder.
        apply(&mut s, Event::Ui(Action::Edit)); apply(&mut s, Event::Key(Key::Char('x'))); apply(&mut s, Event::Key(Key::Esc)); assert_eq!(s.ui.config.name, "RamonsDev");
        apply(&mut s, Event::Ui(Action::Edit)); for _ in 0..30 { apply(&mut s, Event::Key(Key::Back)); } apply(&mut s, Event::Key(Key::Enter));
        assert!(render_lines(&s, 42)[1].contains("⌁ - - ⌁"));
        // Max length.
        apply(&mut s, Event::Ui(Action::Edit)); for _ in 0..40 { apply(&mut s, Event::Key(Key::Char('a'))); } apply(&mut s, Event::Key(Key::Enter));
        assert_eq!(s.ui.config.name.chars().count(), NAME_MAX);
        // Hiding every tab still leaves CONFIG; a config that hides MAIN starts elsewhere.
        let ui = Ui::new(Config { hidden: vec![Tab::Main, Tab::Git], ..Config::default() }, None); assert_eq!(ui.tab, Tab::Prof);
    }
    #[test]
    fn the_volume_bar_maps_each_cell_to_the_percentage_it_sets() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        s.media.spotify = crate::media::Spotify { state: "reproduciendo".into(), linked: true, running: true, device_id: Some("d".into()), volume: Some(50), ..Default::default() };
        let f = frame(&s, 42, 0);
        let (x, row) = at(&f, "vol");
        for cell in 0..VOLUME_CELLS { assert_eq!(f.hits.iter().filter(|h| h.row == row && (h.x0..h.x1).contains(&(x + 4 + cell))).min_by_key(|h| h.x1 - h.x0).map(|h| h.action.clone()), Some(Action::SpotifyVolume(volume_percent(cell))), "cell {cell} of {}", plain(&f.lines[row])); }
        assert!(plain(&f.lines[row]).contains("50%"), "the bar states the volume the account reports: {:?}", plain(&f.lines[row]));
        apply(&mut s, Event::Ui(Action::SpotifyVolume(volume_percent(7))));
        assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Volume(volume_percent(7))));
        // Without a device there is nothing to turn up: the bar is drawn but answers nothing.
        s.media.spotify.running = false; s.media.spotify.device_id = None; s.ui.spotify_cmd = None;
        let f = frame(&s, 42, 0);
        let (x, row) = at(&f, "vol");
        assert_eq!(click(&f, x + 4, row), None);
    }
    #[test]
    fn shuffle_and_repeat_rows_send_the_state_that_follows_the_click() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        s.media.spotify = crate::media::Spotify { state: "reproduciendo".into(), linked: true, running: true, device_id: Some("d".into()), shuffle: true, repeat: crate::media::Repeat::Context, ..Default::default() };
        let f = frame(&s, 42, 0);
        at(&f, "⇄ sí"); at(&f, "⟲ lista");
        let (x, row) = at(&f, "⇄");
        assert_eq!(click(&f, x, row), Some(Action::SpotifyShuffle(false)));
        let (x, row) = at(&f, "⟲");
        assert_eq!(click(&f, x, row), Some(Action::SpotifyRepeat(crate::media::Repeat::Track)));
        apply(&mut s, Event::Ui(Action::SpotifyShuffle(false)));
        assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Shuffle(false)));
        apply(&mut s, Event::Ui(Action::SpotifyRepeat(crate::media::Repeat::Track)));
        assert_eq!(s.ui.spotify_cmd, Some(SpotifyCmd::Repeat(crate::media::Repeat::Track)));
        // An idle device offers no toggles: they would only write to nothing.
        s.media.spotify.device_id = None; s.ui.spotify_cmd = None;
        let f = frame(&s, 42, 0);
        assert!(f.hits.iter().all(|h| !matches!(h.action, Action::SpotifyShuffle(_) | Action::SpotifyRepeat(_))));
    }
    #[test]
    fn the_track_that_is_playing_is_marked_and_still_playable() {
        let mut s = sample(); s.ui.tab = Tab::Media;
        let track = |id: &str, title: &str| crate::media::Track { id: id.into(), title: title.into(), artist: "Daft Punk".into(), position: 0 };
        s.media.spotify = crate::media::Spotify {
            state: "reproduciendo".into(), linked: true, running: true, device_id: Some("d".into()),
            playlists: vec![crate::media::Playlist { id: "p".into(), name: "Rock".into(), cover: None }],
            selected: Some(0),
            tracks: vec![track("a", "Uno"), track("b", "Dos"), track("c", "Tres")],
            playing_id: Some("b".into()),
            ..Default::default()
        };
        let row = |f: &Frame, needle: &str| f.lines.iter().map(|l| plain(l)).find(|l| l.contains(needle)).unwrap_or_else(|| panic!("no {needle:?}"));
        let f = frame(&s, 42, 0);
        assert!(row(&f, "Dos").contains("▶"), "the playing track is marked: {:?}", row(&f, "Dos"));
        assert!(!row(&f, "Uno").contains("▶") && !row(&f, "Tres").contains("▶"), "and only that one");
        // The mark never steals the click: the marked row still plays its own index.
        let (x, y) = at(&f, "Dos");
        assert_eq!(click(&f, x, y), Some(Action::SpotifyPlay(1)));
        // A track played from search is not in this list, so no row is marked.
        s.media.spotify.playing_id = Some("zzz".into());
        let f = frame(&s, 42, 0);
        assert!(["Uno", "Dos", "Tres"].iter().all(|t| !row(&f, t).contains("▶")), "an unknown id marks nothing");
    }
}


