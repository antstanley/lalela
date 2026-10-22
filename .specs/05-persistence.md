# 05 — Persistence

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io

This page defines everything Lalela writes or reads outside memory. That covers the saved audio + transcript pair, the sidecar format, reopening a pair, the settings file, and API keys in the Keychain. The shapes are formalised in [canonical-types.schema.json](canonical-types.schema.json). The entities are defined in [01-domain-model.md](01-domain-model.md).

---

## Responsibilities

1. Plan a save as a pure function: target names, collisions, whether to copy audio, whether anything is replaced.
2. Execute the plan with atomic writes so that a failed save never leaves a partial file under a final name.
3. Find and validate a saved sidecar when the user opens audio or a sidecar.
4. Persist `Settings` as JSON and API keys in the Keychain.

---

## Saved layout

A save writes three files side by side into the output folder:

```
<output folder>/
├── interview.m4a                 copy of the source audio, byte-for-byte
├── interview.txt                 plain transcript, UTF-8, one segment per line, trailing \n
└── interview.transcript.json     sidecar: metadata + timed segments
```

| File | Name | Content |
|---|---|---|
| Audio | `<stem>.<original extension>` | Unmodified copy of `AudioSource.path` |
| Text | `<stem>.txt` | `Transcript::text()` followed by `\n` |
| Sidecar | `<stem>.transcript.json` | `TranscriptDocument` (below), pretty-printed with 2-space indentation |

---

## Save planning — `plan_save`

`lalela-core/src/save.rs`:

```rust
pub fn plan_save(
    audio: &AudioSource,
    source_folder: &Path,                 // canonicalised by the caller
    output_folder: &Path,                 // canonicalised by the caller
    names_in_folder: &BTreeSet<String>,   // from TranscriptStore::list_names
) -> Result<SavePlan, SaveError>;

pub struct SavePlan {
    pub folder: PathBuf,
    pub stem: FileStem,
    pub audio_target: String,
    pub text_target: String,
    pub sidecar_target: String,
    pub copy_audio: bool,
    pub replaces_existing: bool,
}
```

Rules, in order:

1. **Stem.** Start from the source file stem. Replace `/`, `:`, NUL, and control characters with `-`, strip leading `.`, and trim whitespace. Truncate to `FILE_STEM_BYTES_MAX` on a character boundary. If the result is empty, use `transcript`.
2. **In-place case.** This applies when the source audio's parent directory is the output folder (the controller canonicalises both paths before calling, so `plan_save` compares them without I/O) **and** the source stem is already a valid `FileStem` without sanitising. The plan keeps the source file name unchanged as `audio_target`, sets `copy_audio = false`, and sets `replaces_existing` when `<stem>.txt` or `<stem>.transcript.json` exists. The UI asks "Replace files" before executing such a plan. Keeping the name unchanged means the sibling lookup on reopen finds the sidecar.
3. **Copy case.** Otherwise, including an in-folder source whose stem needed sanitising, `copy_audio = true`. If any of the three names exist, try `<stem> 2`, `<stem> 3`, and so on, up to `SAVE_NAME_ATTEMPTS_MAX`. All three files share the suffix. When the attempts run out, return `SaveError::NoFreeName`. `replaces_existing` is always false in this case.

`SaveError` has two variants. `NoFreeName` is returned when the suffixes run out. `OutputFolderNotAbsolute` is returned when the folder path is relative; the settings store only stores absolute paths, so this one marks a Lalela bug.

The plan is a pure function of its inputs, so its tests need no filesystem.

---

## Save execution — `TranscriptStore`

```rust
pub trait TranscriptStore: Send + Sync {
    fn list_names(&self, folder: &Path) -> Result<BTreeSet<String>, StoreError>;
    fn execute(&self, plan: &SavePlan, audio: &AudioSource, transcript: &Transcript,
               observer: &dyn TranscriptionObserver) -> Result<SavedPair, StoreError>;
    fn read_sidecar(&self, path: &Path) -> Result<(Transcript, AudioMetadata), StoreError>;
}

pub enum StoreError {
    FolderMissing, FolderNotWritable, FolderTooLarge, Conflict,
    SidecarTooLarge, UnsupportedVersion, Corrupt { detail: String },
    Cancelled, Io { detail: String },
}
```

`lalela-app` builds one `FsTranscriptStore` and shares it as `Arc<dyn TranscriptStore>`. `list_names` runs on the main thread during Save, and `read_sidecar` during Open. They are bounded by `FOLDER_ENTRIES_SCAN_MAX` and `SIDECAR_BYTES_MAX`, so the frame they block stays short. `execute` moves file bytes and runs only on the worker thread:

```
encode the sidecar; if encoded length > SIDECAR_BYTES_MAX → StoreError::SidecarTooLarge (nothing written)
for each file in [audio (if copy_audio), sidecar, text]:
    write to "<folder>/.<target>.lalela-tmp"        (audio: COPY_CHUNK_BYTES chunks, progress per chunk)
    file.sync_all()
    rename tmp → target                              (atomic within one volume)
    record target in `written`
on any error:
    remove every tmp file
    remove every target in `written` that this save created (not ones it replaced)
    return the error
sync the folder
```

- `list_names` reads at most `FOLDER_ENTRIES_SCAN_MAX` entries. A larger folder returns `StoreError::FolderTooLarge` and does not save a partial plan.
- Before executing, the store re-checks that no planned name appeared since planning, unless `replaces_existing` is set. A race yields `StoreError::Conflict`, and the controller re-plans once.
- The encode-time size check means every sidecar Lalela writes also passes `read_sidecar`'s size check.
- The audio copy reports progress as `Progress::Uploading { sent, total }`, reusing the bytes-moved variant, and checks cancellation per chunk.
- The output folder must exist and be writable. The store does not create it.

---

## Sidecar format — `TranscriptDocument`

The `audio` object is `AudioMetadata`. It identifies the saved audio file so that reopening can match the sidecar to it.

```json
{
  "schema_version": 1,
  "id": "trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f",
  "created_at_unix_ms": 1792575000000,
  "audio": { "file_name": "interview.m4a", "size_bytes": 48213344, "duration_ms": 2530110 },
  "provider": "deepgram",
  "model": "nova-3",
  "language": "en",
  "text": "Welcome back to the show.\nToday we're talking about field recording.",
  "segments": [
    { "start_ms": 0, "end_ms": 2140, "text": "Welcome back to the show." },
    { "start_ms": 2480, "end_ms": 5310, "text": "Today we're talking about field recording." }
  ]
}
```

Decoding (`read_sidecar`) is strict:

1. File size ≤ `SIDECAR_BYTES_MAX`. Read, then `serde_json` decode with `deny_unknown_fields`.
2. `schema_version == 1`. Any other value is `StoreError::UnsupportedVersion`.
3. Convert through the core's validating constructors (`TranscriptId::try_new`, `ProviderId::from_str`, `Transcript::from_parts`), so all six transcript invariants are checked again.
4. Check that `text` equals the text derived from `segments`. A mismatch is `StoreError::Corrupt`, because the `.txt` and the sidecar must agree.

---

## Reopening

`classify_open_path(path, sibling_sidecar_exists: bool) -> Result<OpenTarget, OpenError>` in `lalela-core/src/open.rs` is a pure classifier. The controller checks whether the sibling sidecar exists and passes the answer in, so the core does no I/O:

| Path | `OpenTarget` | Controller then |
|---|---|---|
| `*.transcript.json` | `Sidecar(path)` | Reads the sidecar, then opens `<folder>/<audio.file_name>` as the audio |
| Allowed audio extension, `sibling_sidecar_exists` | `AudioWithSidecar { audio, sidecar }` | Opens the audio, then reads the sidecar |
| Allowed audio extension, no sibling | `AudioOnly(audio)` | Opens the audio only; state `Loaded` |
| Anything else | `Err(OpenError::UnsupportedFile)` | Shows the error |

The I/O failures that follow classification belong to the controller (`lalela-app`), as `OpenFailure`: `FileNotFound`, `AudioTooLarge`, `AudioMissing` (a sidecar's audio file is absent), and `Player(PlayerError)`. Each becomes a status-line message, and the session stays where it was.

When a sidecar is present:

- If the audio's size differs from the sidecar's `AudioMetadata.size_bytes`, or its probed duration differs from `audio.duration_ms` by more than `SEGMENT_END_TOLERANCE_MS`, the sidecar is ignored. The status line reads "Saved transcript does not match this audio," and the session opens as `Loaded`.
- If the sidecar is invalid, the session opens as `Loaded` and the status line names the problem. The audio still plays.
- If it is valid, the session opens as `Saved` with a `SavedPair` for the folder and stem.

---

## Settings — `JsonSettingsStore`

Path: `~/Library/Application Support/Lalela/settings.json`. The directory is created on first save. The file shape is `SettingsDocument`.

```rust
pub trait SettingsStore {
    fn load(&self) -> Result<Settings, SettingsError>;
    fn save(&self, settings: &Settings) -> Result<(), SettingsError>;
}
pub enum SettingsError { NotFound, Invalid { detail: String }, Io { detail: String } }
```

```json
{
  "schema_version": 1,
  "output_folder": "/Users/me/Transcripts",
  "provider": "apple-speech",
  "models": { "apple-speech": "speech-transcriber", "mistral-voxtral": "voxtral-mini-2602", "deepgram": "nova-3" },
  "language": "auto"
}
```

- Written atomically (temp file plus rename) whenever the user changes a setting.
- Read once at startup, capped at `SETTINGS_BYTES_MAX`. A missing file (`NotFound`) yields defaults. An invalid file is renamed to `settings.json.invalid-<unix_ms>`, defaults are used, and the status line says so. The app never crashes on bad settings.
- A well-formed model id that is not in the provider's current model list falls back to that provider's `model_default`. An unknown provider string fails the schema and makes the file `Invalid`.

---

## API keys — `KeychainSecretStore`

| Item | Value |
|---|---|
| Keychain item class | Generic password |
| Service | `io.senzo.lalela` |
| Account | Provider string form: `mistral-voxtral`, `deepgram` |
| API | `security_framework::passwords::{get,set,delete}_generic_password` |

```rust
pub trait SecretStore {
    fn get(&self, provider: ProviderId) -> Result<Option<StoredKey>, SecretError>;
    fn set(&self, provider: ProviderId, key: &ApiKey) -> Result<(), SecretError>;
    fn delete(&self, provider: ProviderId) -> Result<(), SecretError>;
}
pub struct StoredKey { pub key: ApiKey, pub source: KeySource }
pub enum KeySource { Environment, Keychain }
pub enum SecretError { Invalid, Denied, Platform { status: i32 } }
```

`SecretStore` is used on the main thread only. `KeychainSecretStore` implements it; the environment check is part of `get`.

- `SecretStore::get(provider)` first checks the environment variable (`MISTRAL_API_KEY`, `DEEPGRAM_API_KEY`). When it is set and valid, it wins. This is the development override. Otherwise it reads the Keychain.
- Values pass through `ApiKey::try_new` on read and on write. An invalid Keychain value is `SecretError::Invalid`, reported and not used. A refused Keychain prompt is `Denied`.
- The settings sheet shows only where the key comes from ("Key saved", "Using MISTRAL_API_KEY from the environment", or "No key") and never displays the stored value. Saving overwrites the item, and Remove deletes it.
- Keys are read on the main thread when a transcription starts, and moved into the job. They are never written to settings, sidecars, or logs.

---

## Assumptions and open questions

**Assumptions**

- The output folder is on a local or mounted volume where `rename` within a directory is atomic.
- Lalela is not sandboxed, so it can write to any folder the user can write to without security-scoped bookmarks.
- Keychain access from a signed bundle does not prompt after the first allow. Unsigned development builds may prompt each launch.

**Decisions**

- *Temp files in the target folder.* **`.<name>.lalela-tmp` beside the target.** A rename is atomic only within one volume, and the system temp directory may be on a different one.
- *Collision handling.* **Numeric suffix shared by all three files.** The three files stay recognisably paired, and nothing is overwritten without an explicit confirmation.
- *Strict sidecar decoding.* **`deny_unknown_fields` and a version gate.** Lalela is the only writer. An unknown field means a newer Lalela or manual tampering, and a strict reject is safer than a partial read.
- *Store calls on the main thread.* **`list_names` and `read_sidecar` run on main; `execute` on the worker.** Planning and the replace confirmation need the folder listing before any job starts, and both reads are capped. Only the unbounded-time work, copying audio, goes to the worker.
- *Encode-time sidecar check.* **Reject an oversize sidecar before writing.** It guarantees that every saved pair reopens, even when JSON escaping inflates a transcript near its text cap.
- *Env override for keys.* **Environment beats Keychain.** Developers and live tests can run without touching the Keychain, and a bundled app launched from Finder has no such variables.
- *Not sandboxed.* **No App Sandbox.** A persisted output folder under the sandbox needs security-scoped bookmarks, and Mac App Store distribution is a non-goal.

**Open questions**

- *Copy versus link.* Large audio files are copied in full. Should Lalela use APFS clones (`clonefile`) when source and target share a volume, which makes the copy instant and space-free?
- *Default output folder.* With no output folder set, Save opens the folder picker. Should there be a default such as `~/Documents/Lalela`?
