//! Every named limit constant in Lalela (08-limits.md).
//!
//! The rule is that every bound is a named constant, named with its units
//! last, and referenced everywhere it applies. Reaching a limit is an
//! observable event that produces a typed error; nothing is silently
//! truncated.

// ---------------------------------------------------------------------------
// Audio input
// ---------------------------------------------------------------------------

/// Maximum size of an opened audio file, in bytes (1 GiB).
pub const AUDIO_BYTES_MAX: u64 = 1 << 30;

/// Maximum probed duration of an opened audio file, in milliseconds (3 h).
pub const AUDIO_DURATION_MS_MAX: u64 = 10_800_000;

// ---------------------------------------------------------------------------
// Transcript
// ---------------------------------------------------------------------------

/// Maximum number of segments in a transcript.
pub const SEGMENTS_COUNT_MAX: usize = 20_000;

/// Maximum UTF-8 bytes in one segment's text.
pub const SEGMENT_TEXT_BYTES_MAX: usize = 16_384;

/// Maximum bytes of the derived plain text (`Transcript::text()`).
pub const TRANSCRIPT_TEXT_BYTES_MAX: usize = 4 * 1024 * 1024; // 4 MiB

/// A segment end may overshoot the audio duration by at most this many
/// milliseconds.
pub const SEGMENT_END_TOLERANCE_MS: u64 = 1_000;

/// Maximum bytes of a `LanguageTag` (the longest accepted shape, `xxx-Xxxx-999`,
/// is 12 bytes).
pub const LANGUAGE_TAG_BYTES_MAX: usize = 16;

/// JSON bytes per segment beyond its text (keys, two integers, punctuation,
/// indentation). Used only in compile-time size relations.
pub const SEGMENT_JSON_OVERHEAD_BYTES_MAX: u64 = 128;

/// The largest media time expressible in `Milliseconds`: the maximum audio
/// duration plus the segment-end tolerance. This is the JSON Schema `maximum`
/// for `Milliseconds` (10 801 000).
pub const MILLISECONDS_MAX_U64: u64 = AUDIO_DURATION_MS_MAX + SEGMENT_END_TOLERANCE_MS;

// ---------------------------------------------------------------------------
// Providers and API keys
// ---------------------------------------------------------------------------

/// Maximum number of providers in the registry.
pub const PROVIDERS_COUNT_MAX: usize = 8;

/// Maximum number of models a provider may offer.
pub const MODELS_PER_PROVIDER_MAX: usize = 8;

/// Minimum length of an API key, in bytes.
pub const API_KEY_BYTES_MIN: usize = 8;

/// Maximum length of an API key, in bytes.
pub const API_KEY_BYTES_MAX: usize = 512;

/// Maximum bytes kept of a provider error detail.
pub const ERROR_DETAIL_BYTES_MAX: usize = 512;

// ---------------------------------------------------------------------------
// Persistence planning
// ---------------------------------------------------------------------------

/// Maximum bytes of a saved file stem (leaves room for ` 99.transcript.json`
/// under the 255-byte APFS name limit).
pub const FILE_STEM_BYTES_MAX: usize = 200;

/// How many numeric collision suffixes to try before giving up.
pub const SAVE_NAME_ATTEMPTS_MAX: u32 = 99;

// ---------------------------------------------------------------------------
// UI and threading
// ---------------------------------------------------------------------------

/// Maximum number of jobs in flight at once; enforced by `SessionState`.
pub const JOBS_IN_FLIGHT_MAX: usize = 1;

/// Command channel capacity: one job plus `Shutdown`.
pub const WORKER_COMMANDS_QUEUE_MAX: usize = 2;

/// Outcome channel capacity: one outcome per job, with one spare.
pub const WORKER_EVENTS_QUEUE_MAX: usize = 2;

/// Join timeout on exit, in milliseconds.
pub const WORKER_SHUTDOWN_WAIT_MS: u64 = 2_000;

/// Maximum number of `UiIntent` values applied per frame.
pub const INTENTS_PER_FRAME_MAX: usize = 32;

/// Repaint interval while playing, in milliseconds (about 30 Hz).
pub const PLAYHEAD_REPAINT_INTERVAL_MS: u64 = 33;

/// Repaint interval while a job runs, in milliseconds.
pub const PROGRESS_REPAINT_INTERVAL_MS: u64 = 250;

/// `←` / `→` seek step, in milliseconds.
pub const SEEK_STEP_MS: u64 = 5_000;

/// `⌥←` restarts the current segment when the playhead is more than this deep
/// into it, in milliseconds.
pub const SEGMENT_RESTART_THRESHOLD_MS: u64 = 2_000;

/// Auto-scroll is suppressed for this long after a user scroll, in milliseconds.
pub const AUTOSCROLL_SUPPRESS_MS: u64 = 3_000;

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

/// Minimum inner window width, in points.
pub const WINDOW_WIDTH_PT_MIN: f32 = 520.0;

/// Minimum inner window height, in points.
pub const WINDOW_HEIGHT_PT_MIN: f32 = 360.0;

/// Width below which the toolbar pickers wrap to a second row, in points.
pub const TOOLBAR_WRAP_WIDTH_PT: f32 = 720.0;

/// Maximum transcript line length, in characters.
pub const TRANSCRIPT_LINE_CHARS_MAX: usize = 80;

/// Transcript font size at launch, in points.
pub const TRANSCRIPT_FONT_PT_DEFAULT: f32 = 15.0;

/// Smallest transcript font size, in points.
pub const TRANSCRIPT_FONT_PT_MIN: f32 = 12.0;

/// Largest transcript font size, in points.
pub const TRANSCRIPT_FONT_PT_MAX: f32 = 24.0;

/// Transcript font size step.
pub const TRANSCRIPT_FONT_PT_STEP: f32 = 1.0;

// ---------------------------------------------------------------------------
// Compile-time relations between limits (08-limits.md §Compile-time relations)
// ---------------------------------------------------------------------------

/// The maximum `Milliseconds` value must match the JSON Schema `maximum`.
const _: () = assert!(MILLISECONDS_MAX_U64 == 10_801_000);
