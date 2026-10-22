//! `FsTranscriptStore`: atomic save execution and sidecar reads
//! (05-persistence.md §Save execution).

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use lalela_core::audio::AudioSource;
use lalela_core::ports::{AudioMetadata, StoreError, TranscriptStore, TranscriptionObserver};
use lalela_core::save::{SavePlan, SavedPair};
use lalela_core::transcript::Transcript;
use lalela_core::{Bytes, Progress};

use crate::limits::{COPY_CHUNK_BYTES, FOLDER_ENTRIES_SCAN_MAX, SIDECAR_BYTES_MAX};
use crate::sidecar::{decode_sidecar, encode_sidecar};

/// A filesystem-backed store.
#[derive(Debug, Default)]
pub struct FsTranscriptStore;

impl FsTranscriptStore {
    /// Build a store.
    #[must_use]
    pub fn new() -> Self {
        Self
    }

    /// The temp name beside a target, on the same volume.
    fn temp_path(folder: &Path, target: &str) -> PathBuf {
        folder.join(format!(".{target}.lalela-tmp"))
    }

    /// Atomically write `bytes` to `final_path`, using a temp file in the
    /// same directory.
    pub(crate) fn write_file_atomic(final_path: &Path, bytes: &[u8]) -> Result<(), StoreError> {
        let folder = final_path.parent().ok_or_else(|| StoreError::Io {
            detail: "target has no parent directory".to_owned(),
        })?;
        let target = final_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| StoreError::Io {
                detail: "target has no file name".to_owned(),
            })?;
        Self::write_atomic(folder, target, bytes)
    }

    /// Whether any name this save will write appeared since the plan was made.
    /// The audio target is excluded when no copy is needed: it is the source
    /// file itself and is not written by this save.
    fn planned_names_exist(plan: &SavePlan) -> bool {
        let mut names = vec![plan.text_target.as_str(), plan.sidecar_target.as_str()];
        if plan.copy_audio {
            names.push(plan.audio_target.as_str());
        }
        names.iter().any(|name| plan.folder.join(name).exists())
    }

    /// Atomically write `bytes` to `folder/target`.
    fn write_atomic(folder: &Path, target: &str, bytes: &[u8]) -> Result<(), StoreError> {
        let final_path = folder.join(target);
        let tmp = Self::temp_path(folder, target);
        let mut file = File::create(&tmp).map_err(|error| StoreError::Io {
            detail: format!("create temp file failed: {error}"),
        })?;
        file.write_all(bytes).map_err(|error| StoreError::Io {
            detail: format!("write temp file failed: {error}"),
        })?;
        file.sync_all().map_err(|error| StoreError::Io {
            detail: format!("sync temp file failed: {error}"),
        })?;
        drop(file);
        fs::rename(&tmp, &final_path).map_err(|error| StoreError::Io {
            detail: format!("rename temp file failed: {error}"),
        })
    }

    /// Copy the audio into `folder/audio_target` in chunks, reporting progress
    /// and checking cancellation.
    fn copy_audio(
        audio: &AudioSource,
        folder: &Path,
        target: &str,
        observer: &dyn TranscriptionObserver,
    ) -> Result<(), StoreError> {
        let final_path = folder.join(target);
        let tmp = Self::temp_path(folder, target);
        let result = (|| -> Result<(), StoreError> {
            let mut source = File::open(&audio.path).map_err(|error| StoreError::Io {
                detail: format!("open audio failed: {error}"),
            })?;
            let mut dest = File::create(&tmp).map_err(|error| StoreError::Io {
                detail: format!("create temp audio failed: {error}"),
            })?;
            let total = audio.size_bytes.as_u64();
            let mut buffer = vec![0u8; COPY_CHUNK_BYTES];
            let mut sent: u64 = 0;
            loop {
                if observer.is_cancelled() {
                    return Err(StoreError::Cancelled);
                }
                let read = source.read(&mut buffer).map_err(|error| StoreError::Io {
                    detail: format!("read audio failed: {error}"),
                })?;
                if read == 0 {
                    break;
                }
                dest.write_all(&buffer[..read])
                    .map_err(|error| StoreError::Io {
                        detail: format!("write audio failed: {error}"),
                    })?;
                sent += read as u64;
                observer.on_progress(Progress::Uploading {
                    sent: Bytes::new(sent),
                    total: Bytes::new(total),
                });
            }
            dest.sync_all().map_err(|error| StoreError::Io {
                detail: format!("sync audio failed: {error}"),
            })?;
            drop(dest);
            fs::rename(&tmp, &final_path).map_err(|error| StoreError::Io {
                detail: format!("rename audio failed: {error}"),
            })
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }
}

impl TranscriptStore for FsTranscriptStore {
    fn list_names(&self, folder: &Path) -> Result<BTreeSet<String>, StoreError> {
        if !folder.is_dir() {
            return Err(StoreError::FolderMissing);
        }
        let entries: Vec<_> = fs::read_dir(folder)
            .map_err(|error| StoreError::Io {
                detail: format!("read_dir failed: {error}"),
            })?
            .collect();
        if entries.len() > FOLDER_ENTRIES_SCAN_MAX {
            return Err(StoreError::FolderTooLarge);
        }
        let mut names = BTreeSet::new();
        for entry in entries {
            let entry = entry.map_err(|error| StoreError::Io {
                detail: format!("read_dir entry failed: {error}"),
            })?;
            if let Some(name) = entry.file_name().to_str() {
                names.insert(name.to_owned());
            }
        }
        Ok(names)
    }

    fn execute(
        &self,
        plan: &SavePlan,
        audio: &AudioSource,
        transcript: &Transcript,
        observer: &dyn TranscriptionObserver,
    ) -> Result<SavedPair, StoreError> {
        if !plan.folder.is_dir() {
            return Err(StoreError::FolderMissing);
        }
        if !plan.replaces_existing && Self::planned_names_exist(plan) {
            return Err(StoreError::Conflict);
        }

        // Encode first: an oversize sidecar fails before anything is written.
        let sidecar_bytes = encode_sidecar(transcript, audio)?;
        if sidecar_bytes.len() as u64 > SIDECAR_BYTES_MAX {
            return Err(StoreError::SidecarTooLarge);
        }
        let text_bytes = {
            let mut text = transcript.text();
            text.push('\n');
            text.into_bytes()
        };

        // Order: audio (when copied), sidecar, text.
        let mut written: Vec<String> = Vec::new();
        let result = (|| -> Result<(), StoreError> {
            if plan.copy_audio {
                Self::copy_audio(audio, &plan.folder, &plan.audio_target, observer)?;
                written.push(plan.audio_target.clone());
            }
            Self::write_atomic(&plan.folder, &plan.sidecar_target, &sidecar_bytes)?;
            written.push(plan.sidecar_target.clone());
            Self::write_atomic(&plan.folder, &plan.text_target, &text_bytes)?;
            written.push(plan.text_target.clone());
            // Sync the folder so the renames are durable.
            let folder = File::open(&plan.folder).map_err(|error| StoreError::Io {
                detail: format!("open folder for sync failed: {error}"),
            })?;
            folder.sync_all().map_err(|error| StoreError::Io {
                detail: format!("folder sync failed: {error}"),
            })?;
            Ok(())
        })();

        if let Err(error) = result {
            // Remove every temp file and every target this save created.
            for name in &written {
                let _ = fs::remove_file(Self::temp_path(&plan.folder, name));
                let _ = fs::remove_file(plan.folder.join(name));
            }
            return Err(error);
        }

        Ok(SavedPair {
            folder: plan.folder.clone(),
            stem: plan.stem.clone(),
            audio_file_name: plan.audio_target.clone(),
            text_file_name: plan.text_target.clone(),
            sidecar_file_name: plan.sidecar_target.clone(),
        })
    }

    fn read_sidecar(&self, path: &Path) -> Result<(Transcript, AudioMetadata), StoreError> {
        let metadata = fs::metadata(path).map_err(|error| StoreError::Io {
            detail: format!("sidecar metadata failed: {error}"),
        })?;
        if metadata.len() > SIDECAR_BYTES_MAX {
            return Err(StoreError::SidecarTooLarge);
        }
        let mut file = File::open(path).map_err(|error| StoreError::Io {
            detail: format!("open sidecar failed: {error}"),
        })?;
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut bytes)
            .map_err(|error| StoreError::Io {
                detail: format!("read sidecar failed: {error}"),
            })?;
        decode_sidecar(&bytes)
    }
}

/// Open a file for syncing (used by tests).
#[allow(dead_code)]
fn _open_for_sync(path: &Path) -> Result<File, StoreError> {
    OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|error| StoreError::Io {
            detail: format!("open failed: {error}"),
        })
}
