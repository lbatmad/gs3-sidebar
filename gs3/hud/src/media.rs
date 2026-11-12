//! MEDIA tab state: Spotify audio in the upper section, YouTube video in the section below it.
//!
//! Playback is delegated to external processes (`librespot` for Spotify audio, `mpv` for YouTube
//! video), so this module holds only what the rail draws and the commands it hands to those
//! processes. It never holds credentials: tokens live in their own private 0600 files.

#[derive(Clone, Debug, PartialEq)]
pub struct Media { pub spotify: Spotify, pub youtube: Youtube }

/// The exact redirect the Spotify app must allowlist: a loopback IPv4 is the only place Spotify
/// accepts plain HTTP, and `localhost` is rejected.
pub const REDIRECT: &str = "http://127.0.0.1:8899/callback";

/// Upper section: the user's own playlists, the selected playlist's ten tracks, transport and seek.
#[derive(Clone, Debug, PartialEq)]
pub struct Spotify {
    /// One line under the section title: `falta el client_id`, `esperando el navegador`, … or the failure reason.
    pub state: String,
    /// A private credential file exists, so the section offers a refresh instead of first-time setup.
    pub linked: bool,
    /// The user's own playlists, as `/me/playlists` returns them.
    pub playlists: Vec<Playlist>,
    /// Which playlist the tracks below belong to.
    pub selected: Option<usize>,
    /// Exactly the ten tracks of the selected playlist.
    pub tracks: Vec<Track>,
    pub results: Vec<SearchTrack>,
    pub search_done: bool,
    pub device_id: Option<String>,
    pub running: bool,
    pub playing: bool,
    pub title: Option<String>,
    pub artist: String,
    pub position_ms: u64,
    pub duration_ms: u64,
    /// `device.volume_percent` from `/me/player`; `None` until a reply names a device.
    pub volume: Option<u64>,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// The `item.id` that `/me/player` reports as playing now, so `⌁ Pistas` can mark its row.
    pub playing_id: Option<String>,
    pub oauth_url: Option<String>,
    pub album_cover: Option<String>,
    pub cover_url: Option<String>,
    pub cover_png: Option<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Playlist { pub id: String, pub name: String, pub cover: Option<String> }

#[derive(Clone, Debug, PartialEq)]
pub struct Track { pub id: String, pub title: String, pub artist: String, pub position: usize }
#[derive(Clone, Debug, PartialEq)]
pub struct SearchTrack { pub id: String, pub title: String, pub artist: String, pub cover: Option<String> }

/// `/me/player` reports repeat as one of these three words; the card cycles them in the same order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Repeat { #[default] Off, Context, Track }
impl Repeat {
    pub fn parse(word: &str) -> Repeat { match word { "context" => Repeat::Context, "track" => Repeat::Track, _ => Repeat::Off } }
    /// What `PUT /me/player/repeat?state=` receives.
    pub fn state(self) -> &'static str { match self { Repeat::Off => "off", Repeat::Context => "context", Repeat::Track => "track" } }
    pub fn cycle(self) -> Repeat { match self { Repeat::Off => Repeat::Context, Repeat::Context => Repeat::Track, Repeat::Track => Repeat::Off } }
    /// The word the card shows; `off` would read as a failure, not as a choice.
    pub fn label(self) -> &'static str { match self { Repeat::Off => "no", Repeat::Context => "lista", Repeat::Track => "tema" } }
}

/// What the rail asks the Spotify engine to do. Sent over a channel, never run on the render path.
#[derive(Clone, Debug, PartialEq)]
pub enum SpotifyCmd {
    /// Consent in the browser, then the code exchange; the rail never sees a password.
    Authorize,
    /// Renew the stored access token from the refresh token.
    Refresh,
    /// List the playlists of the authorized profile.
    Playlists,
    /// Show the ten tracks of this playlist id, taken from `playlists`.
    Select(String),
    Search(String),
    PlayResult(usize),
    Start,
    Play(usize),
    Pause,
    Resume,
    Next,
    Previous,
    Seek(f64),
    /// Volume as a percentage, already clamped to 0–100 by the bar that produced the click.
    Volume(u64),
    Shuffle(bool),
    Repeat(Repeat),
}

/// Lower section: YouTube search, transport and seek. The video is drawn by `mpv`, never here.
#[derive(Clone, Debug, PartialEq)]
pub struct Youtube {
    /// One line under the section title: `sin reproducción`, or the failure reason.
    pub state: String,
    /// What mpv reports as the playing title, once something is playing.
    pub title: Option<String>,
    /// Seconds played and total seconds, as reported by mpv.
    pub position: f64,
    pub duration: f64,
    pub playing: bool,
    /// The queue the HUD owns: the seed result followed by its YouTube recommendations (the RD mix).
    pub queue: Vec<Video>,
    /// Index into `queue`; only meaningful while `queue` is not empty.
    pub index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Video { pub id: String, pub title: String }

/// A rectangle in terminal cells, relative to the containing grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellRect { pub x: u32, pub y: u32, pub width: u32, pub height: u32 }
impl CellRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self { Self { x, y, width, height } }
}

/// What the rail asks the YouTube engine to do. Sent over a channel, never run on the render path.
#[derive(Clone, Debug, PartialEq)]
pub enum MediaCmd {
    /// Pause when playing, resume when paused.
    Toggle,
    Next,
    Prev,
    /// Absolute position as a fraction of the current duration (0.0–1.0), from a click on the bar.
    Seek(f64),
    /// Where the rail is drawing the video block, or `None` when it is not on screen at all: another
    /// tab, a collapsed card or one scrolled out of view. The window follows this rectangle.
    CardRect(Option<CellRect>),
    /// Search this text, then queue the best match with its recommendations and start it.
    Play(String, Option<CellRect>),
}

impl Default for Spotify {
    fn default() -> Self {
        Spotify { state: "falta el client_id".into(), linked: false, playlists: Vec::new(), selected: None, tracks: Vec::new(), results: Vec::new(), search_done: false, device_id: None, running: false, playing: false, title: None, artist: String::new(), position_ms: 0, duration_ms: 0, volume: None, shuffle: false, repeat: Repeat::Off, playing_id: None, oauth_url: None, album_cover: None, cover_url: None, cover_png: None }
    }
}

impl Default for Media {
    fn default() -> Self {
        Media { spotify: Spotify::default(), youtube: Youtube::default() }
    }
}

impl Default for Youtube {
    fn default() -> Self {
        Youtube { state: "sin reproducción".into(), title: None, position: 0.0, duration: 0.0, playing: false, queue: Vec::new(), index: 0 }
    }
}
