//! Filesystem and JSON adapters: `FsTranscriptStore` and `JsonSettingsStore`
//! (architecture-principles.md §Crate roles).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod limits;
pub mod settings;
pub mod sidecar;
pub mod store;

pub use settings::{JsonSettingsStore, SettingsDocument};
pub use sidecar::{
    AudioMetadataDto, SegmentDto, TranscriptDocument, decode_sidecar, encode_sidecar,
};
pub use store::FsTranscriptStore;
