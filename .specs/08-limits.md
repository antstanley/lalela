# 08 — Limits

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io

This page lists every named limit constant in Lalela with its value, units, and rationale. The rule that every bound is a named constant is in [development-guidelines.md](development-guidelines.md#limits-and-bounds). All constants live in `crates/lalela-core/src/limits.rs`, apart from the HTTP and shim constants, which live in the crate that applies them and are re-exported for tests.

Reaching any of these limits is an error the user sees. Nothing is silently truncated.

---

## Audio input

| Constant | Value | Bounds | Rationale |
|---|---|---|---|
| `AUDIO_BYTES_MAX` | 1 073 741 824 (1 GiB) | Size of an opened audio file | Covers three hours of high-bitrate audio. Keeps uploads and copies finite. |
| `AUDIO_DURATION_MS_MAX` | 10 800 000 (3 h) | Probed duration of an opened file | Matches Voxtral 2's documented ceiling and keeps transcripts within `SEGMENTS_COUNT_MAX`. |
| `VOXTRAL_AUDIO_BYTES_MAX` | 104 857 600 (100 MiB) | Voxtral direct upload | Well below the 512 MB Files API cap; Mistral does not document a direct-upload cap (see Open questions). |
| `VOXTRAL_AUDIO_DURATION_MS_MAX` | 10 800 000 (3 h) | Voxtral audio duration | Voxtral 2 documented ceiling. |

## Transcript

| Constant | Value | Bounds | Rationale |
|---|---|---|---|
| `SEGMENTS_COUNT_MAX` | 20 000 | Segments per transcript | Three hours at one segment per 0.54 s; real output is 2–5 s per segment. |
| `SEGMENT_TEXT_BYTES_MAX` | 16 384 | UTF-8 bytes per segment | Long enough for a Voxtral segment, short enough to reject a runaway one. |
| `TRANSCRIPT_TEXT_BYTES_MAX` | 4 194 304 (4 MiB) | Derived plain text | Three hours of speech is about 200 KiB. |
| `SEGMENT_END_TOLERANCE_MS` | 1 000 | Segment end beyond audio duration | Providers round segment ends; see [01-domain-model.md](01-domain-model.md). |
| `LANGUAGE_TAG_BYTES_MAX` | 16 | `LanguageTag` length | The longest accepted shape, `xxx-Xxxx-999`, is 12 bytes. |
| `SEGMENT_JSON_OVERHEAD_BYTES_MAX` | 128 | JSON bytes per segment beyond its text (keys, two integers, punctuation, indentation) | Used only in the `const` size relations below. |

## Providers and HTTP

| Constant | Value | Bounds | Rationale |
|---|---|---|---|
| `PROVIDERS_COUNT_MAX` | 8 | Registry size | Three are registered. The bound keeps registry iteration finite. |
| `MODELS_PER_PROVIDER_MAX` | 8 | `ProviderDescriptor.models` | Picker length. |
| `API_KEY_BYTES_MIN` | 8 | `ApiKey` length | Rejects pasted fragments. |
| `API_KEY_BYTES_MAX` | 512 | `ApiKey` length | Longer than any provider key format. |
| `HTTP_TIMEOUT_CONNECT_MS` | 10 000 | TCP + TLS connect | — |
| `HTTP_TIMEOUT_SEND_BODY_MS` | 1 800 000 (30 min) | Upload | 1 GiB at about 5 Mbit/s. |
| `HTTP_TIMEOUT_RECEIVE_MS` | 900 000 (15 min) | Wait for the response after upload | Above Deepgram's 10-minute processing window. |
| `HTTP_TIMEOUT_GLOBAL_MS` | 2 820 000 (47 min) | Whole request | Connect + send + receive + margin. |
| `HTTP_RESPONSE_BYTES_MAX` | 33 554 432 (32 MiB) | Response body | Deepgram word arrays for three hours are about 8 MiB. |
| `HTTP_RETRY_ATTEMPTS_MAX` | 2 | Extra attempts after the first | Three tries in total. |
| `HTTP_RETRY_BACKOFF_MS` | `[1 000, 4 000]` | Delay before retry n | One entry per attempt; `const` assert that the length equals `HTTP_RETRY_ATTEMPTS_MAX`. |
| `HTTP_RETRY_AFTER_MS_MAX` | 30 000 | Honoured `Retry-After` | Longer waits fail as `RateLimited`. |
| `HTTP_CANCEL_POLL_MS` | 100 | Cancellation check interval during retry sleeps | Cancel feels immediate. |
| `ERROR_DETAIL_BYTES_MAX` | 512 | Provider error text kept in `TranscribeError` | Enough to diagnose without storing bodies. |

## Shim

| Constant | Value | Bounds | Rationale |
|---|---|---|---|
| `SHIM_SPEECH_OUTPUT_BYTES_MAX` | 16 777 216 (16 MiB) | `out_json_capacity` | `const` assert that it is ≥ `TRANSCRIPT_TEXT_BYTES_MAX + SEGMENTS_COUNT_MAX × SEGMENT_JSON_OVERHEAD_BYTES_MAX` (6.4 MiB). |
| `SPEECH_TIMEOUT_MS` | 3 600 000 (60 min) | `lalela_speech_transcribe` wait | Upper bound for a three-hour file at 3× real time; the measured value is an Open question. |
| `SPEECH_CANCEL_POLL_MS` | 250 | Cancellation check interval in the shim | Cancel feels immediate without busy polling. |
| `PLAYERS_OPEN_MAX` | 1 | Shim player slots | One audio file per session. |

## Persistence

| Constant | Value | Bounds | Rationale |
|---|---|---|---|
| `FILE_STEM_BYTES_MAX` | 200 | Saved file stem | Leaves room for ` 99.transcript.json` under the 255-byte APFS name limit. |
| `SAVE_NAME_ATTEMPTS_MAX` | 99 | Collision suffixes tried | Beyond 99 copies the folder needs tidying, not more suffixes. |
| `FOLDER_ENTRIES_SCAN_MAX` | 100 000 | Entries read by `list_names` | Bounds the directory scan. |
| `SIDECAR_BYTES_MAX` | 16 777 216 (16 MiB) | Sidecar write and read | `const` assert that it is ≥ `2 × TRANSCRIPT_TEXT_BYTES_MAX + SEGMENTS_COUNT_MAX × SEGMENT_JSON_OVERHEAD_BYTES_MAX` (10.4 MiB), covering `text` plus segment texts plus structure. JSON escaping can exceed this, so `execute` also checks the encoded size before writing. |
| `SETTINGS_BYTES_MAX` | 65 536 | Settings read | — |
| `COPY_CHUNK_BYTES` | 1 048 576 (1 MiB) | Audio copy buffer | Progress and cancellation granularity. |

## UI and threading

| Constant | Value | Bounds | Rationale |
|---|---|---|---|
| `WORKER_COMMANDS_QUEUE_MAX` | 2 | Command channel | One job plus `Shutdown`. |
| `WORKER_EVENTS_QUEUE_MAX` | 2 | Outcome channel | One outcome per job, with one spare. |
| `JOBS_IN_FLIGHT_MAX` | 1 | Concurrent jobs | Enforced by `SessionState`. |
| `WORKER_SHUTDOWN_WAIT_MS` | 2 000 | Join on exit | Quitting stays prompt. |
| `INTENTS_PER_FRAME_MAX` | 32 | `UiIntent`s per frame | Far above any real frame. |
| `PLAYHEAD_REPAINT_INTERVAL_MS` | 33 | Repaint while playing | About 30 Hz. |
| `PROGRESS_REPAINT_INTERVAL_MS` | 250 | Repaint while a job runs | Elapsed-time label. |
| `SEEK_STEP_MS` | 5 000 | ← / → seek | Common media-player step. |
| `SEGMENT_RESTART_THRESHOLD_MS` | 2 000 | ⌥← restarts the current segment | Matches track-skip behaviour. |
| `AUTOSCROLL_SUPPRESS_MS` | 3 000 | Auto-scroll pause after a user scroll | — |

## Layout

| Constant | Value | Bounds | Rationale |
|---|---|---|---|
| `WINDOW_WIDTH_PT_MIN` | 520 | Window inner width | The toolbar fits on two rows. |
| `WINDOW_HEIGHT_PT_MIN` | 360 | Window inner height | Toolbar, player bar, and at least five transcript rows. |
| `TOOLBAR_WRAP_WIDTH_PT` | 720 | Width below which the pickers wrap to a second row | Labels never truncate. |
| `TRANSCRIPT_LINE_CHARS_MAX` | 80 | Transcript line length | Readability. |
| `TRANSCRIPT_FONT_PT_DEFAULT` | 15 | Transcript font size at launch | — |
| `TRANSCRIPT_FONT_PT_MIN` | 12 | Smallest transcript size | — |
| `TRANSCRIPT_FONT_PT_MAX` | 24 | Largest transcript size | — |
| `TRANSCRIPT_FONT_PT_STEP` | 1 | ⌘= / ⌘− step | Twelve steps across the range. |

---

## Assumptions and open questions

**Assumptions**

- APFS limits file names to 255 UTF-8 bytes.
- Provider limits change. The values above reflect provider documentation as of 2026-10-21.

**Decisions**

- *One page for values.* **All concrete limits on this page.** Development guidelines hold the meta-rule. Collecting values here makes a review of every bound a single read.
- *Compile-time relations.* **`const` assertions for relations between limits.** The shim buffer and the sidecar cap must hold a maximum transcript, and the backoff table must match the attempt count. A bad edit then fails to compile.

**Open questions**

- *Voxtral limits.* What are Mistral's real direct-upload size and duration limits for Voxtral 3? `VOXTRAL_AUDIO_BYTES_MAX` is a placeholder.
- *Speech timeout.* What does `SPEECH_TIMEOUT_MS` need to be, based on measured on-device throughput?
