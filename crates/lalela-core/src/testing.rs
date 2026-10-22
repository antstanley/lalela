//! Fakes for tests (development-guidelines.md §Testing).
//!
//! This module exists only to back tests; `expect`/`unwrap` are acceptable
//! here because the values are constructed by the fakes themselves.

#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::path::Path;
use std::sync::Mutex;

use crate::id::TranscriptId;
use crate::ports::{AudioPlayer, Clock, IdSource, PlayerError, PlayheadState};
use crate::units::Milliseconds;

/// A fake player with an injectable position. It never touches the filesystem.
#[derive(Debug)]
pub struct FakePlayer {
    loaded: bool,
    duration_ms: u64,
    position_ms: u64,
    playing: bool,
}

impl FakePlayer {
    /// A player with nothing loaded.
    #[must_use]
    pub fn new() -> Self {
        Self {
            loaded: false,
            duration_ms: 0,
            position_ms: 0,
            playing: false,
        }
    }

    /// Advance the playhead by `ms` when playing, like a running clock.
    pub fn advance(&mut self, ms: u64) {
        if !self.playing {
            return;
        }
        self.position_ms = (self.position_ms + ms).min(self.duration_ms);
        if self.position_ms >= self.duration_ms {
            self.playing = false;
        }
    }
}

impl Default for FakePlayer {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioPlayer for FakePlayer {
    fn load(&mut self, _path: &Path) -> Result<Milliseconds, PlayerError> {
        self.loaded = true;
        self.duration_ms = 60_000;
        self.position_ms = 0;
        self.playing = false;
        Ok(Milliseconds::try_new(self.duration_ms).expect("fake duration is valid"))
    }

    fn play(&mut self) -> Result<(), PlayerError> {
        if !self.loaded {
            return Err(PlayerError::NotLoaded);
        }
        if self.position_ms >= self.duration_ms {
            self.position_ms = 0;
        }
        self.playing = true;
        Ok(())
    }

    fn pause(&mut self) -> Result<(), PlayerError> {
        if !self.loaded {
            return Err(PlayerError::NotLoaded);
        }
        self.playing = false;
        Ok(())
    }

    fn seek(&mut self, position: Milliseconds) -> Result<(), PlayerError> {
        if !self.loaded {
            return Err(PlayerError::NotLoaded);
        }
        self.position_ms = position.as_u64().min(self.duration_ms);
        Ok(())
    }

    fn state(&self) -> Result<PlayheadState, PlayerError> {
        if !self.loaded {
            return Err(PlayerError::NotLoaded);
        }
        Ok(PlayheadState {
            position: Milliseconds::try_new(self.position_ms).expect("fake position is valid"),
            is_playing: self.playing,
        })
    }

    fn unload(&mut self) {
        self.loaded = false;
        self.duration_ms = 0;
        self.position_ms = 0;
        self.playing = false;
    }
}

/// A clock with an injectable current time.
#[derive(Debug)]
pub struct FakesClock {
    now_ms: Mutex<u64>,
}

impl FakesClock {
    /// A clock at `now_ms`.
    #[must_use]
    pub fn new(now_ms: u64) -> Self {
        Self {
            now_ms: Mutex::new(now_ms),
        }
    }

    /// Set the current time.
    pub fn set(&self, now_ms: u64) {
        *self.now_ms.lock().expect("clock lock") = now_ms;
    }
}

impl Clock for FakesClock {
    fn now_unix_ms(&self) -> u64 {
        *self.now_ms.lock().expect("clock lock")
    }
}

/// An ID source that returns valid, deterministic UUIDv7-shaped IDs.
#[derive(Debug)]
pub struct FakeIdSource {
    next: Mutex<u64>,
}

impl FakeIdSource {
    /// An ID source starting at counter 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next: Mutex::new(0),
        }
    }
}

impl Default for FakeIdSource {
    fn default() -> Self {
        Self::new()
    }
}

impl IdSource for FakeIdSource {
    fn next_transcript_id(&self) -> TranscriptId {
        // The last hex digit encodes the counter; every other character is a
        // fixed, valid UUIDv7.
        let base = "01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f";
        let mut next = self.next.lock().expect("id source lock");
        let digit = "0123456789abcdef".as_bytes()[(*next % 16) as usize] as char;
        let mut chars: Vec<char> = base.chars().collect();
        let last = chars.len() - 1;
        chars[last] = digit;
        let id = format!("trn_{}", chars.into_iter().collect::<String>());
        *next += 1;
        TranscriptId::try_new(id).expect("fake id is valid")
    }
}
