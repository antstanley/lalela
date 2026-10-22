//! Persistence limits (08-limits.md §Persistence).

use lalela_core::limits::{SEGMENTS_COUNT_MAX, TRANSCRIPT_TEXT_BYTES_MAX};

/// Maximum sidecar file size, read or written (16 MiB).
pub const SIDECAR_BYTES_MAX: u64 = 16 * 1024 * 1024;
/// Maximum settings file size (64 KiB).
pub const SETTINGS_BYTES_MAX: u64 = 65_536;
/// Audio copy buffer size for progress and cancellation granularity (1 MiB).
pub const COPY_CHUNK_BYTES: usize = 1024 * 1024;
/// Maximum directory entries scanned by `list_names`.
pub const FOLDER_ENTRIES_SCAN_MAX: usize = 100_000;

/// The shim speech buffer constant, kept in the store for the compile-time
/// relation below.
pub const SHIM_SPEECH_OUTPUT_BYTES_MAX: u64 = 16 * 1024 * 1024;

const _: () = assert!(
    SHIM_SPEECH_OUTPUT_BYTES_MAX
        >= TRANSCRIPT_TEXT_BYTES_MAX as u64 + (SEGMENTS_COUNT_MAX as u64) * 128,
    "shim buffer must hold a maximum-size transcript"
);

const _: () = assert!(
    SIDECAR_BYTES_MAX >= 2 * (TRANSCRIPT_TEXT_BYTES_MAX as u64) + (SEGMENTS_COUNT_MAX as u64) * 128,
    "sidecar cap must hold a maximum-size transcript plus structure"
);
