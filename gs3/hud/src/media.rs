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
    pub device_id: Option<String>,
    pub running: bool,
    pub playing: bool,
    pub oauth_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Playlist { pub id: String, pub name: String }

#[derive(Clone, Debug, PartialEq)]
pub struct Track { pub title: String, pub artist: String, pub position: usize }

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
    Start,
    Play(usize),
    Pause,
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

/// What the rail asks the YouTube engine to do. Sent over a channel, never run on the render path.
#[derive(Clone, Debug, PartialEq)]
pub enum MediaCmd {
    /// Pause when playing, resume when paused.
    Toggle,
    Next,
    Prev,
    /// Absolute position as a fraction of the current duration (0.0–1.0), from a click on the bar.
    Seek(f64),
    /// Search this text, then queue the best match with its recommendations and start it.
    Play(String),
}

impl Default for Spotify {
    fn default() -> Self {
        Spotify { state: "falta el client_id".into(), linked: false, playlists: Vec::new(), selected: None, tracks: Vec::new(), device_id: None, running: false, playing: false, oauth_url: None }
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
