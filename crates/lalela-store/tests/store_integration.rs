//! Integration tests for the filesystem store (development-guidelines.md
//! Integration tests may use unwrap/expect freely.
#![allow(clippy::unwrap_used, clippy::expect_used)]
//! §Testing).

use lalela_core::audio::{AudioFormat, AudioSource};
use lalela_core::id::TranscriptId;
use lalela_core::ports::{ProviderId, StoreError, TranscriptStore};
use lalela_core::save::{FileStem, plan_save};
use lalela_core::transcript::{Segment, Transcript};
use lalela_core::units::{Bytes, LanguageTag, Milliseconds, ModelName};
use lalela_store::FsTranscriptStore;

fn audio_in(dir: &std::path::Path, name: &str, size: u64) -> AudioSource {
    let path = dir.join(name);
    std::fs::write(&path, vec![7u8; size as usize]).unwrap();
    AudioSource::try_new(
        path,
        AudioFormat::M4a,
        Bytes::new(size),
        Milliseconds::try_new(60_000).unwrap(),
    )
    .unwrap()
}

fn transcript() -> Transcript {
    Transcript::from_parts(
        TranscriptId::try_new("trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f".to_owned()).unwrap(),
        ProviderId::Deepgram,
        ModelName::try_new("nova-3".to_owned()).unwrap(),
        Some(LanguageTag::try_new("en".to_owned()).unwrap()),
        1_000,
        Milliseconds::try_new(60_000).unwrap(),
        vec![Segment {
            start_ms: Milliseconds::try_new(0).unwrap(),
            end_ms: Milliseconds::try_new(2_140).unwrap(),
            text: "Welcome back.".to_owned(),
        }],
    )
    .unwrap()
}

struct NoopObserver;

impl lalela_core::ports::TranscriptionObserver for NoopObserver {
    fn on_progress(&self, _progress: lalela_core::ports::Progress) {}
    fn is_cancelled(&self) -> bool {
        false
    }
}

#[test]
fn save_then_reopen_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let source = audio_in(dir.path(), "interview.m4a", 128);
    let store = FsTranscriptStore::new();
    let names = store.list_names(dir.path()).unwrap();
    let plan = plan_save(&source, dir.path(), dir.path(), &names).unwrap();
    assert!(!plan.copy_audio);
    let pair = store
        .execute(&plan, &source, &transcript(), &NoopObserver)
        .unwrap();
    assert_eq!(pair.audio_file_name, "interview.m4a");
    assert!(dir.path().join("interview.txt").exists());
    assert!(dir.path().join("interview.transcript.json").exists());

    let sidecar_path = dir.path().join("interview.transcript.json");
    let (loaded, metadata) = store.read_sidecar(&sidecar_path).unwrap();
    assert_eq!(loaded.text(), "Welcome back.");
    assert_eq!(metadata.file_name, "interview.m4a");
    assert_eq!(metadata.duration_ms.as_u64(), 60_000);
}

#[test]
fn list_names_counts_and_bounds() {
    let dir = tempfile::tempdir().unwrap();
    let store = FsTranscriptStore::new();
    assert_eq!(store.list_names(dir.path()).unwrap().len(), 0);
    std::fs::write(dir.path().join("a.txt"), b"x").unwrap();
    assert!(store.list_names(dir.path()).unwrap().contains("a.txt"));
    assert_eq!(
        store.list_names(&dir.path().join("missing")),
        Err(StoreError::FolderMissing)
    );
}

#[test]
fn copy_case_writes_all_three_files() {
    let dir = tempfile::tempdir().unwrap();
    let source_dir = tempfile::tempdir().unwrap();
    let source = audio_in(source_dir.path(), "interview.m4a", 256);
    let store = FsTranscriptStore::new();
    let names = store.list_names(dir.path()).unwrap();
    let plan = plan_save(&source, source_dir.path(), dir.path(), &names).unwrap();
    assert!(plan.copy_audio);
    let pair = store
        .execute(&plan, &source, &transcript(), &NoopObserver)
        .unwrap();
    assert!(dir.path().join("interview.m4a").exists());
    assert!(dir.path().join("interview.txt").exists());
    assert!(dir.path().join("interview.transcript.json").exists());
    assert_eq!(
        pair.stem,
        FileStem::try_new("interview".to_owned()).unwrap()
    );
}

#[test]
fn conflict_after_planning_is_detected() {
    let dir = tempfile::tempdir().unwrap();
    let source = audio_in(dir.path(), "interview.m4a", 64);
    let store = FsTranscriptStore::new();
    let names = store.list_names(dir.path()).unwrap();
    let plan = plan_save(&source, dir.path(), dir.path(), &names).unwrap();
    assert!(!plan.copy_audio);
    std::fs::write(dir.path().join("interview.txt"), b"x").unwrap();
    let result = store.execute(&plan, &source, &transcript(), &NoopObserver);
    assert_eq!(result, Err(StoreError::Conflict));
}

#[test]
fn oversized_sidecar_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.transcript.json");
    let big_sidecar = vec![b'x'; 17 * 1024 * 1024];
    std::fs::write(&path, &big_sidecar).unwrap();
    let store = FsTranscriptStore::new();
    assert!(matches!(
        store.read_sidecar(&path),
        Err(StoreError::SidecarTooLarge)
    ));
}

#[test]
fn corrupt_sidecar_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.transcript.json");
    std::fs::write(&path, b"{ not json").unwrap();
    let store = FsTranscriptStore::new();
    assert!(matches!(
        store.read_sidecar(&path),
        Err(StoreError::Corrupt { .. })
    ));
}
