//! Hand-written mirrors of `shim/include/lalela_shim.h`
//! (03-macos-shim.md §ABI).

use std::ffi::c_void;

/// The shim status code mirror. The discriminants must match
/// `shim/include/lalela_shim.h`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShimStatus {
    /// 0: success.
    Ok,
    /// 1: null pointer, zero capacity, non-UTF-8 path, or wrong thread.
    InvalidArgument,
    /// 2: path does not exist or is unreadable.
    FileNotFound,
    /// 3: AVFoundation cannot decode the file.
    UnsupportedAudio,
    /// 4: Speech is unavailable.
    SpeechUnavailable,
    /// 5: no supported locale is equivalent to the request.
    LocaleUnsupported,
    /// 6: on-device assets could not be installed.
    AssetsUnavailable,
    /// 7: cancelled.
    Cancelled,
    /// 8: timeout elapsed.
    TimedOut,
    /// 9: output JSON exceeds the buffer capacity.
    OutputTooLarge,
    /// 10: player call with no open player.
    NoPlayer,
    /// 11: analyser refused for concurrency.
    InsufficientResources,
    /// 99: any other thrown error.
    Internal,
    /// Any value not in the table.
    Unknown(i32),
}

impl ShimStatus {
    /// Interpret a raw `lalela_status` value.
    #[must_use]
    pub fn from_raw(value: i32) -> Self {
        match value {
            0 => Self::Ok,
            1 => Self::InvalidArgument,
            2 => Self::FileNotFound,
            3 => Self::UnsupportedAudio,
            4 => Self::SpeechUnavailable,
            5 => Self::LocaleUnsupported,
            6 => Self::AssetsUnavailable,
            7 => Self::Cancelled,
            8 => Self::TimedOut,
            9 => Self::OutputTooLarge,
            10 => Self::NoPlayer,
            11 => Self::InsufficientResources,
            99 => Self::Internal,
            other => Self::Unknown(other),
        }
    }

    /// Whether the status is `Ok`.
    #[must_use]
    pub const fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }
}

pub type ProgressFn = unsafe extern "C" fn(*mut c_void, u32, u32);
pub type CancelledFn = unsafe extern "C" fn(*mut c_void) -> bool;

unsafe extern "C" {
    pub fn lalela_speech_is_available() -> bool;
    pub fn lalela_speech_transcribe(
        audio_path: *const std::ffi::c_char,
        locale_identifier: *const std::ffi::c_char,
        timeout_ms: u64,
        context: *mut c_void,
        on_progress: ProgressFn,
        is_cancelled: CancelledFn,
        out_json: *mut u8,
        out_json_capacity: u32,
        out_json_length: *mut u32,
    ) -> i32;

    pub fn lalela_player_open(
        audio_path: *const std::ffi::c_char,
        out_duration_ms: *mut u64,
    ) -> i32;
    pub fn lalela_player_play() -> i32;
    pub fn lalela_player_pause() -> i32;
    pub fn lalela_player_seek(position_ms: u64) -> i32;
    pub fn lalela_player_state(out_position_ms: *mut u64, out_is_playing: *mut bool) -> i32;
    pub fn lalela_player_close();

    pub fn lalela_appearance_accent_rgba(out_rgba: *mut u8) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The status table (03-macos-shim.md §Status codes), asserted against the
    /// hand-written mirror so the two cannot drift.
    #[test]
    fn discriminantants_match_the_spec_table() {
        assert_eq!(ShimStatus::Ok, ShimStatus::from_raw(0));
        assert_eq!(ShimStatus::InvalidArgument, ShimStatus::from_raw(1));
        assert_eq!(ShimStatus::FileNotFound, ShimStatus::from_raw(2));
        assert_eq!(ShimStatus::UnsupportedAudio, ShimStatus::from_raw(3));
        assert_eq!(ShimStatus::SpeechUnavailable, ShimStatus::from_raw(4));
        assert_eq!(ShimStatus::LocaleUnsupported, ShimStatus::from_raw(5));
        assert_eq!(ShimStatus::AssetsUnavailable, ShimStatus::from_raw(6));
        assert_eq!(ShimStatus::Cancelled, ShimStatus::from_raw(7));
        assert_eq!(ShimStatus::TimedOut, ShimStatus::from_raw(8));
        assert_eq!(ShimStatus::OutputTooLarge, ShimStatus::from_raw(9));
        assert_eq!(ShimStatus::NoPlayer, ShimStatus::from_raw(10));
        assert_eq!(ShimStatus::InsufficientResources, ShimStatus::from_raw(11));
        assert_eq!(ShimStatus::Internal, ShimStatus::from_raw(99));
        assert_eq!(ShimStatus::Unknown(12), ShimStatus::from_raw(12));
        assert_eq!(ShimStatus::Unknown(-1), ShimStatus::from_raw(-1));
    }

    #[test]
    fn ok_is_ok_only_for_value_zero() {
        assert!(ShimStatus::Ok.is_ok());
        assert!(!ShimStatus::FileNotFound.is_ok());
        assert!(!ShimStatus::Unknown(0).is_ok());
    }
}
