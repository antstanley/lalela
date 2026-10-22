//! Platform adapters: `SystemClock` and `UuidV7IdSource` (06-app-shell.md
//! §Supporting ports).

use lalela_core::id::TranscriptId;
use lalela_core::ports::{Clock, IdSource};

/// The wall clock.
#[derive(Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_ms(&self) -> u64 {
        let duration = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| panic!("system clock is before the Unix epoch"));
        duration.as_millis() as u64
    }
}

/// A UUIDv7 transcript ID source.
#[derive(Debug, Default)]
pub struct UuidV7IdSource;

impl IdSource for UuidV7IdSource {
    fn next_transcript_id(&self) -> TranscriptId {
        let uuid = uuid::Uuid::now_v7();
        TranscriptId::try_new(format!("trn_{uuid}"))
            .unwrap_or_else(|| panic!("uuid v7 is a valid transcript id"))
    }
}
