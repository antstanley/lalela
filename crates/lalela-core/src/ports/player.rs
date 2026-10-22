//! The `AudioPlayer` port (04-playback.md).

use std::path::Path;

use crate::units::Milliseconds;

/// The player's current position and playing state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayheadState {
    /// Position in milliseconds from the start of the audio.
    pub position: Milliseconds,
    /// Whether audio is currently playing.
    pub is_playing: bool,
}

/// A playback failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerError {
    /// No player is loaded.
    NotLoaded,
    /// The file does not exist or is unreadable.
    FileNotFound,
    /// AVFoundation cannot decode the file.
    UnsupportedAudio,
    /// A shim-internal failure.
    Platform { status: i32 },
}

/// The playback port. There is deliberately no `Send` bound: the real adapter
/// (`AvAudioPlayer`) is `!Send` and lives on the main thread.
pub trait AudioPlayer {
    /// Load `path` and report its duration in milliseconds.
    fn load(&mut self, path: &Path) -> Result<Milliseconds, PlayerError>;

    /// Start or resume playback.
    fn play(&mut self) -> Result<(), PlayerError>;

    /// Pause playback.
    fn pause(&mut self) -> Result<(), PlayerError>;

    /// Seek to `position`, clamped to `[0, duration]`.
    fn seek(&mut self, position: Milliseconds) -> Result<(), PlayerError>;

    /// The current position and playing state.
    fn state(&self) -> Result<PlayheadState, PlayerError>;

    /// Unload the current file.
    fn unload(&mut self);
}
