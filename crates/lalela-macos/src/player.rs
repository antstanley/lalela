//! `AvAudioPlayer`: the AVFoundation player via the shim
//! (03-macos-shim.md §Player).

use std::ffi::{CString, c_void};
use std::marker::PhantomData;
use std::path::Path;

use lalela_core::ports::{AudioPlayer, PlayerError, PlayheadState};
use lalela_core::units::Milliseconds;

/// The AVFoundation-backed player. Deliberately `!Send`: `PhantomData<*const ()>`
/// keeps it on the main thread.
pub struct AvAudioPlayer {
    _not_send: PhantomData<*const ()>,
}

impl AvAudioPlayer {
    /// Build a player. Must be created and used on the main thread.
    #[must_use]
    pub fn new() -> Self {
        Self {
            _not_send: PhantomData,
        }
    }
}

impl Default for AvAudioPlayer {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a shim status to a `PlayerError` (03-macos-shim.md §Player).
fn map_player_error(status: crate::ffi::ShimStatus) -> PlayerError {
    match status {
        crate::ffi::ShimStatus::Ok => PlayerError::NotLoaded, // unreachable for errors
        crate::ffi::ShimStatus::NoPlayer => PlayerError::NotLoaded,
        crate::ffi::ShimStatus::FileNotFound => PlayerError::FileNotFound,
        crate::ffi::ShimStatus::UnsupportedAudio => PlayerError::UnsupportedAudio,
        crate::ffi::ShimStatus::InvalidArgument => {
            panic!("player call with a valid argument was rejected (wrong thread?)")
        }
        other => PlayerError::Platform {
            status: match other {
                crate::ffi::ShimStatus::Internal => 99,
                crate::ffi::ShimStatus::Unknown(status) => status,
                _ => 99,
            },
        },
    }
}

impl AudioPlayer for AvAudioPlayer {
    fn load(&mut self, path: &Path) -> Result<Milliseconds, PlayerError> {
        let path = CString::new(path.as_os_str().as_encoded_bytes())
            .map_err(|_| PlayerError::FileNotFound)?;
        let mut duration: u64 = 0;
        // SAFETY: `path` is a valid NUL-terminated C string; `duration` is a
        // caller-owned out-pointer.
        let status = unsafe { crate::ffi::lalela_player_open(path.as_ptr(), &mut duration) };
        let status = crate::ffi::ShimStatus::from_raw(status);
        if !status.is_ok() {
            return Err(map_player_error(status));
        }
        Milliseconds::try_new(duration).ok_or(PlayerError::Platform { status: 99 })
    }

    fn play(&mut self) -> Result<(), PlayerError> {
        // SAFETY: FFI call with no pointers.
        let status = unsafe { crate::ffi::lalela_player_play() };
        let status = crate::ffi::ShimStatus::from_raw(status);
        if status.is_ok() {
            Ok(())
        } else {
            Err(map_player_error(status))
        }
    }

    fn pause(&mut self) -> Result<(), PlayerError> {
        // SAFETY: FFI call with no pointers.
        let status = unsafe { crate::ffi::lalela_player_pause() };
        let status = crate::ffi::ShimStatus::from_raw(status);
        if status.is_ok() {
            Ok(())
        } else {
            Err(map_player_error(status))
        }
    }

    fn seek(&mut self, position: Milliseconds) -> Result<(), PlayerError> {
        // SAFETY: FFI call with a value argument only.
        let status = unsafe { crate::ffi::lalela_player_seek(position.as_u64()) };
        let status = crate::ffi::ShimStatus::from_raw(status);
        if status.is_ok() {
            Ok(())
        } else {
            Err(map_player_error(status))
        }
    }

    fn state(&self) -> Result<PlayheadState, PlayerError> {
        let mut position: u64 = 0;
        let mut playing = false;
        // SAFETY: both out-pointers are caller-owned.
        let status = unsafe { crate::ffi::lalela_player_state(&mut position, &mut playing) };
        let status = crate::ffi::ShimStatus::from_raw(status);
        if !status.is_ok() {
            return Err(map_player_error(status));
        }
        Ok(PlayheadState {
            position: Milliseconds::try_new(position)
                .ok_or(PlayerError::Platform { status: 99 })?,
            is_playing: playing,
        })
    }

    fn unload(&mut self) {
        // SAFETY: FFI call with no pointers; idempotent.
        unsafe { crate::ffi::lalela_player_close() };
    }
}

/// Silence the unused import when no c_void usage remains in this module.
#[allow(dead_code)]
fn _keep_c_void(_: *mut c_void) {}
