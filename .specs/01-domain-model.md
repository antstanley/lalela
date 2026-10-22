# 01 — Domain Model

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io

This page defines the entities Lalela's core manages: what they carry, how they are identified, which invariants hold, and the session state machine. The persisted shapes are formalised in [canonical-types.schema.json](canonical-types.schema.json). All types live in `crates/lalela-core/src/`.

---

## ID scheme

Lalela has one persistent entity ID:

```
trn_<uuid7>      e.g. trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f
```

- `trn` identifies a `Transcript`.
- The UUID is a lowercase, hyphenated version 7 UUID (RFC 9562). Its time ordering makes sidecars sort by creation.
- IDs come from the `IdSource` port. The core validates the format in `TranscriptId::try_new` and never generates IDs itself.

| Prefix | Entity |
|---|---|
| `trn_` | Transcript |

`ProviderId` is not an entity ID. It is a closed enum with stable string forms:

| Variant | String form | Display name |
|---|---|---|
| `ProviderId::AppleSpeech` | `apple-speech` | Apple (on-device) |
| `ProviderId::MistralVoxtral` | `mistral-voxtral` | Mistral Voxtral |
| `ProviderId::Deepgram` | `deepgram` | Deepgram |

---

## Value types

| Type | Representation | Invariant |
|---|---|---|
| `Milliseconds` | `u64` | ≤ `AUDIO_DURATION_MS_MAX` + `SEGMENT_END_TOLERANCE_MS` wherever it measures media time |
| `Bytes` | `u64` | — (call sites bound it, e.g. `AudioSource.size_bytes`) |
| `LanguageTag` | `String` | BCP-47 shape: 2–3 lowercase letter language subtag, optional 4-letter titlecase script subtag, optional 2-letter uppercase or 3-digit region subtag; ≤ `LANGUAGE_TAG_BYTES_MAX` |
| `LanguagePreference` | enum `Auto` \| `Tag(LanguageTag)` | — |
| `ModelOption` | `&'static ModelOption` from a `ProviderDescriptor` | Selection only: must belong to the selected provider's model list |
| `ModelName` | `String` | Stored on a `Transcript`. Lowercase `[a-z0-9.-]`, starting with a letter or digit, 1–64 bytes. Not checked against the current model list, so sidecars from retired models still open |
| `ApiKey` | `String`, redacted `Debug`/`Display` | `API_KEY_BYTES_MIN` ≤ length ≤ `API_KEY_BYTES_MAX`; printable ASCII; no whitespace |
| `AudioFormat` | enum `Mp3`, `M4a`, `Mp4`, `Aac`, `Wav`, `Aiff`, `Caf`, `Flac` | Derived from the lowercase file extension; maps to one MIME type |
| `FileStem` | `String` | Non-empty; ≤ `FILE_STEM_BYTES_MAX`; no `/`, `:`, NUL, or control characters; no leading `.` |

---

## Entities

### AudioSource

The audio file the session is working on. Built by `AudioSource::try_new` after the player has loaded the file and reported its duration.

Carries:

- `path` — absolute `PathBuf` of the file as opened.
- `file_name` — the final path component, valid UTF-8.
- `format` — `AudioFormat`.
- `size_bytes` — `Bytes`; > 0 and ≤ `AUDIO_BYTES_MAX`.
- `duration_ms` — `Milliseconds`; > 0 and ≤ `AUDIO_DURATION_MS_MAX`.

### TranscriptDraft

What a `Transcriber` returns: normalised segments without identity. It is never shown or saved directly.

Carries:

- `segments` — `Vec<Segment>` in provider order.
- `language` — `Option<LanguageTag>`: the language the provider used or detected, if it reports one.

### Transcript (`trn_`)

A validated transcript of one audio file. It has two constructors, and both run the same private `Transcript::validate`:

- `Transcript::from_draft(draft, &audio, provider, model, id, created_at_unix_ms)` builds a new transcript from a provider's `TranscriptDraft`.
- `Transcript::from_parts(...)` rebuilds one from a decoded sidecar ([05-persistence.md](05-persistence.md)).

Carries:

- `id` — `TranscriptId`.
- `provider` — `ProviderId`.
- `model` — `ModelName`, the model id as sent to the provider.
- `language` — `Option<LanguageTag>`.
- `created_at_unix_ms` — `u64` from the `Clock` port.
- `audio_duration_ms` — `Milliseconds`, copied from the `AudioSource`.
- `segments` — `Vec<Segment>`.

The plain text is derived, not stored: `Transcript::text()` joins the segment texts with `\n`. The `.txt` file and the sidecar's `text` field both come from it.

Invariants, checked by `validate` on both construction paths:

1. `1 ≤ segments.len() ≤ SEGMENTS_COUNT_MAX`.
2. For each segment, `start_ms ≤ end_ms`.
3. For each segment, `end_ms ≤ audio_duration_ms + SEGMENT_END_TOLERANCE_MS`. `from_draft` clamps an `end_ms` inside the tolerance to `audio_duration_ms` and rejects one beyond it.
4. `start_ms` is non-decreasing across the list.
5. Each segment's text is trimmed, non-empty, contains no line break, and is ≤ `SEGMENT_TEXT_BYTES_MAX`. Adapters replace internal line breaks with a space during normalisation, which keeps the `.txt` at one segment per line.
6. `text().len() ≤ TRANSCRIPT_TEXT_BYTES_MAX`.

A draft that breaks an invariant yields `TranscribeError::MalformedResponse`. It does not panic, because the data came from a third party.

### Segment

One timed span of speech.

Carries:

- `start_ms` — `Milliseconds` from the start of the audio.
- `end_ms` — `Milliseconds`.
- `text` — `String`.

Segment granularity is provider-defined: one final result (Apple), one returned segment (Voxtral), or one sentence (Deepgram). [02-transcription-providers.md](02-transcription-providers.md) records each mapping.

### AudioMetadata

The identity of a saved audio file, stored in the sidecar and returned by `TranscriptStore::read_sidecar` so that reopening can check that the sidecar matches the audio.

Carries:

- `file_name` — `String`, the audio file's name in the same folder.
- `size_bytes` — `Bytes`; > 0 and ≤ `AUDIO_BYTES_MAX`.
- `duration_ms` — `Milliseconds`.

### SavedPair

Where a transcript lives on disk after a save, or where it was found on reopen.

Carries:

- `folder` — absolute `PathBuf`.
- `stem` — `FileStem`.
- `audio_file_name`, `text_file_name`, `sidecar_file_name` — `String`.

### SavePlan

The pure output of `plan_save` in [05-persistence.md](05-persistence.md): target names, whether the audio needs copying, and whether existing files would be replaced.

### Settings

User preferences that persist between launches.

Carries:

- `output_folder` — `Option<PathBuf>`.
- `provider` — the `ProviderId` last selected in the toolbar.
- `models` — one model id per `ProviderId`, resolved to a `ModelOption` on load.
- `language` — `LanguagePreference`.

API keys are not part of `Settings`. They live in the `SecretStore`.

---

## Relationships

```
Settings ──selects──▶ ProviderId ──has 1..*──▶ ModelOption
                                      │
AudioSource 1 ──transcribed by──▶ Transcriber ──▶ TranscriptDraft
     │                                                  │ validated + identified
     │ 1                                                ▼
     └──────────────────────────────────────────── 0..1 Transcript ──1..*── Segment
                                                        │
                                                   0..1 SavedPair  (on disk)
```

A session holds at most one `AudioSource` and at most one current `Transcript`. A successful re-transcription replaces the current transcript. A failed or cancelled one restores it.

---

## Session state machine

`SessionState` is an enum in `lalela-core/src/session.rs`. Every transition goes through `SessionState::apply(event: SessionEvent) -> Result<SessionState, TransitionError>`. An illegal transition is a programmer error in the controller, and the controller asserts against it. `TransitionError { from: StateKind, event: EventKind }` names both sides for the assertion message.

```
  open (audio, no valid sidecar)            open (valid saved pair)
  Empty/Loaded/Unsaved*/Saved ──▶ Loaded    Empty/Loaded/Unsaved*/Saved ──▶ Saved

  Loaded ───┐
  Unsaved* ─┼─ transcribe ─▶ Transcribing ─ succeeded ─▶ Unsaved ─ save ─▶ Saving ─ saved ─▶ Saved
  Saved ────┘                  │       │                    ▲                  │
                               │       └─ failed ─▶ prior   └───── failed ─────┘
                             cancel
                               ▼
                           Cancelling ─ job ended (any outcome) ─▶ prior

  *        after the user confirms discarding the unsaved transcript
  prior    the state the transcription started from, restored with its transcript:
           Loaded, Unsaved(transcript), or Saved(transcript, pair)
```

| State | Holds | Meaning |
|---|---|---|
| `Empty` | — | No audio open |
| `Loaded` | `AudioSource` | Audio open, no transcript |
| `Transcribing` | `AudioSource`, `JobId`, `ProviderId`, `Prior` | A transcription job is running |
| `Cancelling` | `AudioSource`, `JobId`, `ProviderId`, `Prior` | Cancel requested; waiting for the worker to stop |
| `Unsaved` | `AudioSource`, `Transcript` | Transcript present, not on disk |
| `Saving` | `AudioSource`, `Transcript`, `JobId` | A save job is running |
| `Saved` | `AudioSource`, `Transcript`, `SavedPair` | Transcript on disk, matching memory |

`Prior` is an enum: `None`, `Unsaved(Transcript)`, or `Saved(Transcript, SavedPair)`. While a job runs, the transcript pane keeps showing the prior transcript, and playback highlighting keeps working.

### Events

| `SessionEvent` | Legal from | Next state |
|---|---|---|
| `Opened(AudioSource)` | `Empty`, `Loaded`, `Unsaved`, `Saved` | `Loaded` |
| `OpenedSaved(AudioSource, Transcript, SavedPair)` | `Empty`, `Loaded`, `Unsaved`, `Saved` | `Saved` |
| `TranscribeStarted(JobId, ProviderId)` | `Loaded`, `Unsaved`, `Saved` | `Transcribing` |
| `CancelRequested` | `Transcribing` | `Cancelling` |
| `Transcribed(JobId, Transcript)` | `Transcribing` | `Unsaved` |
| `TranscribeEnded(JobId, TranscribeError)` | `Transcribing` | prior; the controller keeps the error for the status line |
| `JobEnded(JobId)` | `Cancelling` | prior; the outcome, success or failure, is discarded |
| `SaveStarted(JobId)` | `Unsaved` | `Saving` |
| `Saved(JobId, SavedPair)` | `Saving` | `Saved` |
| `SaveFailed(JobId, StoreError)` | `Saving` | `Unsaved`; the controller keeps the error for the status line |

Transition rules:

- **Open** is illegal while a job runs (`Transcribing`, `Cancelling`, `Saving`). From `Unsaved` the controller asks the user to confirm before applying it.
- **Transcribe** from `Unsaved` asks the user to confirm, because a successful run replaces the unsaved transcript. A `Saved` transcript stays on disk either way.
- **Job events** carry their `JobId`. An event whose `JobId` does not match the current state's `JobId` is a programmer error.
- **Playback** is independent of these states. It works whenever an `AudioSource` is present.

---

## Required query patterns

| Query | Access pattern |
|---|---|
| Segment under the playhead | Binary search on `start_ms` (`segment_at_position`), O(log n) per frame; see [04-playback.md](04-playback.md) |
| Next / previous segment from the playhead | Same search, ±1 index |
| Saved sidecar for an audio file | Sibling-file lookup `<stem>.transcript.json` in the audio's folder |
| Free target names in a folder | `TranscriptStore::list_names(folder)` once per save, then pure planning |

---

## Assumptions and open questions

**Assumptions**

- Providers return segments in time order or close to it. Lalela sorts nothing; out-of-order segments are rejected.
- Media positions fit in `u64` milliseconds with room to spare, given `AUDIO_DURATION_MS_MAX`.

**Decisions**

- *Creation time.* **`created_at_unix_ms: u64`, not an RFC 3339 string.** It avoids a date-formatting dependency and is unambiguous. The sidecar is for machines, and the `.txt` is for people.
- *Plain text derivation.* **Segments joined by newlines, not the provider's own `text` field.** One source of truth means the `.txt` and the highlighted view can never disagree.
- *Out-of-order segments.* **Rejected, not sorted.** Overlapping or reordered segments indicate diarisation or a decoding bug. Lalela requests neither diarisation nor overlapping output.
- *End-time tolerance.* **Clamp within `SEGMENT_END_TOLERANCE_MS`, reject beyond.** Providers round segment ends and can overshoot the decoded duration slightly; a larger overshoot means the transcript belongs to different audio.
- *Cancelling state.* **Explicit `Cancelling` state.** A blocking HTTP read cannot be interrupted mid-response, so the UI must wait for the worker before starting another job.
- *Failed re-transcription.* **Restore the prior state and transcript.** A failed or cancelled run says nothing about the previous transcript, so losing it would throw away paid or slow work for no reason.
- *Model name on transcripts.* **`ModelName`, not a `ModelOption` reference.** The provider's model list changes between releases, and a sidecar must still open after its model is retired.

**Open questions**

- *Confidence.* Apple and Deepgram report per-word or per-segment confidence. Should `Segment` carry it, so the UI can mark low-confidence text?
