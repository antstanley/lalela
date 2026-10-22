//! Ports: the interfaces between the core and the outside world
//! (architecture-principles.md §Ports).

pub mod player;
pub mod store;
pub mod support;
pub mod transcriber;

pub use player::{AudioPlayer, PlayerError, PlayheadState};
pub use store::{AudioMetadata, StoreError, TranscriptStore};
pub use support::{
    Appearance, AppearanceError, Clock, IdSource, KeySource, SecretError, SecretStore, Settings,
    SettingsError, SettingsStore, StoredKey,
};
pub use transcriber::{
    ModelOption, Progress, ProviderDescriptor, ProviderId, Readiness, TranscribeError, Transcriber,
    TranscriptionObserver, TranscriptionRequest,
};
