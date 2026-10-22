//! The only crate with `unsafe`: the Swift shim FFI and the macOS adapters
//! (architecture-principles.md §Crate roles).

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod appearance;
pub mod ffi;
pub mod keychain;
pub mod player;
pub mod speech;

pub use appearance::MacAppearance;
pub use keychain::KeychainSecretStore;
pub use player::AvAudioPlayer;
pub use speech::{AppleSpeechTranscriber, SHIM_SPEECH_OUTPUT_BYTES_MAX};
