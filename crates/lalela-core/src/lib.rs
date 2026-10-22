//! Lalela core: the domain, the use cases, and the ports.
//!
//! This crate holds everything that is true about Lalela regardless of the
//! platform: the entities and their invariants, the session state machine, save
//! planning, open classification, playback queries, the provider registry, and
//! the port traits that adapters implement. It has no I/O and no threads; the
//! only dependency is `thiserror` (architecture-principles.md §What goes
//! where).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod audio;
pub mod id;
pub mod limits;
pub mod open;
pub mod playback;
pub mod ports;
pub mod registry;
pub mod save;
pub mod session;
pub mod testing;
pub mod transcript;
pub mod units;

pub use audio::{AudioFormat, AudioSource, AudioSourceError};
pub use id::TranscriptId;
pub use ports::*;
pub use save::{FileStem, SaveError, SavePlan, SavedPair, plan_save};
pub use session::{JobId, Prior, SessionEvent, SessionState, StateKind, TransitionError};
pub use transcript::{PartsError, Segment, Transcript, TranscriptDraft};
pub use units::{ApiKey, Bytes, LanguagePreference, LanguageTag, Milliseconds, ModelName};
