//! MEDIA tab state: Spotify audio in the upper section, YouTube video in the section below it.
//!
//! Playback is delegated to external processes (`librespot` for Spotify audio, `mpv` for YouTube
//! video), so this module holds only what the rail draws and the commands it hands to those
//! processes. It never holds credentials: tokens live in their own private 0600 files.

#[derive(Clone, Debug, PartialEq)]
pub struct Media { pub spotify: Spotify, pub youtube: Youtube }

/// Upper section: the user's own playlists, the selected playlist's ten tracks, transport and seek.
#[derive(Clone, Debug, PartialEq)]
pub struct Spotify {
    /// One line under the section title: `sin sesión`, `conectando`, … or the failure reason.
    pub state: String,
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

impl Default for Media {
    fn default() -> Self {
        Media { spotify: Spotify { state: "sin sesión".into() }, youtube: Youtube::default() }
    }
}

impl Default for Youtube {
    fn default() -> Self {
        Youtube { state: "sin reproducción".into(), title: None, position: 0.0, duration: 0.0, playing: false, queue: Vec::new(), index: 0 }
    }
}
