//! The `TranscriptStore` port (05-persistence.md).

use std::collections::BTreeSet;
use std::path::Path;

use crate::audio::AudioSource;
use crate::ports::transcriber::TranscriptionObserver;
use crate::save::{SavePlan, SavedPair};
use crate::transcript::Transcript;
use crate::units::Bytes;
use crate::units::Milliseconds;

/// Identity of the saved audio file a sidecar belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioMetadata {
    /// The audio file's name in the same folder.
    pub file_name: String,
    /// The audio file's size in bytes.
    pub size_bytes: Bytes,
    /// The audio file's duration in milliseconds.
    pub duration_ms: Milliseconds,
}

/// A persistence failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StoreError {
    /// The output folder does not exist.
    FolderMissing,
    /// The output folder is not writable.
    FolderNotWritable,
    /// The output folder has more than `FOLDER_ENTRIES_SCAN_MAX` entries.
    FolderTooLarge,
    /// A planned name appeared since planning.
    Conflict,
    /// The sidecar file is larger than `SIDECAR_BYTES_MAX`.
    SidecarTooLarge,
    /// The sidecar schema version is not supported.
    UnsupportedVersion,
    /// The sidecar is corrupt.
    Corrupt { detail: String },
    /// The save was cancelled.
    Cancelled,
    /// An I/O failure.
    Io { detail: String },
}

/// The port that persists saved pairs and reads sidecars.
pub trait TranscriptStore: Send + Sync {
    /// List file names in `folder`, bounded by `FOLDER_ENTRIES_SCAN_MAX`.
    fn list_names(&self, folder: &Path) -> Result<BTreeSet<String>, StoreError>;

    /// Execute a save plan, writing the audio copy, sidecar, and text file
    /// atomically. Runs on the worker thread.
    fn execute(
        &self,
        plan: &SavePlan,
        audio: &AudioSource,
        transcript: &Transcript,
        observer: &dyn TranscriptionObserver,
    ) -> Result<SavedPair, StoreError>;

    /// Read and strictly validate a sidecar.
    fn read_sidecar(&self, path: &Path) -> Result<(Transcript, AudioMetadata), StoreError>;
}
