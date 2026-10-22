# 02 — Transcription Providers

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io

This page defines the `Transcriber` port, the provider registry, and the three adapters behind them: Apple on-device Speech, Mistral Voxtral, and Deepgram. The domain types they produce are in [01-domain-model.md](01-domain-model.md). The Swift side of the Apple adapter is in [03-macos-shim.md](03-macos-shim.md).

---

## Responsibilities

1. Present every provider to the core through one port, so that the controller, the UI, and the tests never branch on a vendor.
2. Turn an audio file into a `TranscriptDraft` of timed segments, reporting progress and honouring cancellation.
3. Translate every vendor failure into a `TranscribeError` variant.
4. Validate every vendor response before it becomes a domain value.

The providers do **not** own: API-key storage (`SecretStore`), choosing a provider (the controller, from `Settings`), assigning IDs or timestamps (`Transcript::from_draft`), or persistence.

---

## The `Transcriber` port

Defined in `lalela-core/src/ports/transcriber.rs`.

```rust
pub trait Transcriber: Send + Sync {
    fn descriptor(&self) -> &'static ProviderDescriptor;
    fn readiness(&self, api_key_present: bool) -> Readiness;
    fn transcribe(
        &self,
        request: &TranscriptionRequest,
        observer: &dyn TranscriptionObserver,
    ) -> Result<TranscriptDraft, TranscribeError>;
}

pub struct ProviderDescriptor {
    pub id: ProviderId,
    pub display_name: &'static str,
    pub requires_api_key: bool,
    pub models: &'static [ModelOption],     // 1..=MODELS_PER_PROVIDER_MAX
    pub model_default: &'static str,
    pub audio_bytes_max: u64,               // ≤ AUDIO_BYTES_MAX
    pub audio_duration_ms_max: u64,         // ≤ AUDIO_DURATION_MS_MAX
}

pub struct ModelOption {
    pub id: &'static str,
    pub label: &'static str,
    pub supports_language_hint: bool,
}

pub struct TranscriptionRequest<'a> {
    pub transcript_id: &'a TranscriptId,   // also seeds the Voxtral multipart boundary
    pub audio: &'a AudioSource,
    pub model: &'a ModelOption,
    pub language: &'a LanguagePreference,
    pub api_key: Option<&'a ApiKey>,
}

pub trait TranscriptionObserver: Sync {
    fn on_progress(&self, progress: Progress);
    fn is_cancelled(&self) -> bool;
}

pub enum Progress {
    PreparingModel { permille: u16 },       // Apple asset download
    Uploading { sent: Bytes, total: Bytes },
    Processing { permille: Option<u16> },   // None = indeterminate
}

pub enum Readiness { Ready, NeedsApiKey, Unavailable { reason: &'static str } }
```

`descriptor` and `readiness` are cheap, non-blocking, and called on the main thread every frame. `readiness` takes only whether a key is present, from the controller's cached key sources, so no Keychain read happens per frame. `transcribe` is blocking and runs only on the worker thread. The registry is shared between the two threads (see [§Provider registry](#provider-registry)), which is why the trait is `Send + Sync`; every adapter is stateless apart from its immutable `HttpClient` or cached availability flag.

`transcribe` asserts that `request.audio.size_bytes ≤ descriptor.audio_bytes_max`, that the audio is within `audio_duration_ms_max`, that `request.model` belongs to `descriptor.models`, and, when `requires_api_key` is true, that `api_key` is present. The controller checks the same conditions first and shows a message, so a violated assertion means the controller is wrong.

### `TranscribeError`

| Variant | Raised when | User message (abridged) |
|---|---|---|
| `Cancelled` | `observer.is_cancelled()` observed | (none; the prior state is restored) |
| `FileUnreadable` | The audio file cannot be opened or read for upload or analysis | "Lalela could not read the audio file." |
| `ProviderUnavailable { reason }` | Apple Speech is unavailable on this Mac, or the system refused an analyser for lack of resources | "<Provider> is not available: <reason>." |
| `Unauthorized` | HTTP 401 / 403 | "<Provider> rejected the API key." |
| `RateLimited` | HTTP 429 after retries | "<Provider> is rate-limiting requests." |
| `PayloadTooLarge` | HTTP 413, or size over descriptor limit | "File is too large for <Provider>." |
| `UnsupportedFormat` | HTTP 415, or shim `UnsupportedAudio` | "<Provider> cannot read this file format." |
| `UnsupportedLanguage` | No supported locale (Apple); HTTP 400 naming `language` | "<Provider> does not support <language>." |
| `ModelUnavailable` | Apple assets cannot be installed; HTTP 404 on model | "The selected model is not available." |
| `NoSpeech` | Valid response with zero non-empty segments | "No speech was found in this file." |
| `Timeout` | Client timeout, HTTP 504, or shim timeout | "<Provider> timed out." |
| `Network { detail }` | Connect, TLS, or I/O failure after retries | "Could not reach <Provider>." |
| `Provider { status, message }` | Any other non-2xx | "<Provider> returned an error (<status>)." |
| `MalformedResponse { detail }` | Decode failure, invariant violation, or output over a size cap | "<Provider> returned an unexpected response." |
| `PlatformFailure { status }` | Shim `Internal` or an unknown shim status | "An internal macOS error occurred (<status>)." |

`message` and `detail` are truncated to `ERROR_DETAIL_BYTES_MAX` and never include request headers.

---

## Provider registry

`ProviderRegistry` (`lalela-core/src/registry.rs`) holds `Box<dyn Transcriber>` values, at most `PROVIDERS_COUNT_MAX`, at most one per `ProviderId`. `lalela-app` builds it once at startup and wraps it in an `Arc`. The controller uses its clone for descriptors and readiness on the main thread, and the worker uses its clone for `transcribe`:

```rust
let registry = Arc::new(ProviderRegistry::new(vec![
    Box::new(AppleSpeechTranscriber::new()),
    Box::new(VoxtralTranscriber::new(http.clone())),
    Box::new(DeepgramTranscriber::new(http.clone())),
])?);
```

The constructor asserts that every `ProviderId` variant is registered exactly once, so a new variant without an adapter fails at startup.

Adding a provider means:

1. Add a `ProviderId` variant and its string form.
2. Create `crates/lalela-<name>/` implementing `Transcriber`, with fixture tests for `decode_response`.
3. Register it in `lalela-app`.
4. Add its string form to the `ProviderId` enum in [canonical-types.schema.json](canonical-types.schema.json).

---

## Transcription flow

```
Controller (main)                        Worker                          Adapter
──────────────────                       ──────                          ───────
check readiness + limits
SessionState → Transcribing
send Job::Transcribe{job_id, ...} ──────▶ observer = ProgressCell+Cancel
                                          adapter.transcribe(req, obs) ─▶ build request
                                                                         stream upload ─ on_progress
                                                                         await response ─ is_cancelled
                                                                         decode_response
                                          ◀──────────────────────────── TranscriptDraft | Err
                                          Transcript::from_draft(...)
◀──── JobOutcome::Transcribed{job_id, Result<Transcript, TranscribeError>}
SessionState → Unsaved | prior (+error)
```

Every adapter follows the same sequence: build the request, send it with progress and cancellation, decode it into DTOs, then normalise to segments. Normalisation trims each segment's text, replaces internal line breaks with a space, and drops empty segments. It converts seconds to milliseconds with `(seconds * 1000.0).round()`, after asserting the value is finite and non-negative.

---

## Apple on-device Speech — `lalela-macos::AppleSpeechTranscriber`

| Item | Value |
|---|---|
| `ProviderId` | `AppleSpeech` |
| API key | Not required |
| Models | `speech-transcriber` ("On-device"), `supports_language_hint: true` |
| `audio_bytes_max` / `audio_duration_ms_max` | `AUDIO_BYTES_MAX` / `AUDIO_DURATION_MS_MAX` |
| Readiness | `Unavailable` when `lalela_speech_is_available()` returned false; otherwise `Ready`. The adapter calls it once in `new()` and caches the result, so `readiness` never crosses the FFI boundary per frame. |

Flow:

1. Resolve the locale identifier. `Auto` sends `""`, which the shim maps to the system locale. `Tag(t)` sends `t`.
2. Call `lalela_speech_transcribe` with a 16 MiB output buffer (`SHIM_SPEECH_OUTPUT_BYTES_MAX`). The shim reports `PreparingModel` while it installs on-device assets, then `Processing` with a determinate fraction.
3. On `Ok`, decode the output JSON (`ShimSpeechOutput`) into `{ locale, segments: [{start_ms, end_ms, text}] }`. On any other status, map it with the table below.
4. `TranscriptDraft.language` is the locale the shim used, which the shim emits in BCP-47 form (`en-US`) and Rust validates as a `LanguageTag`.

| `ShimStatus` | `TranscribeError` |
|---|---|
| `Ok` | — (decode the output) |
| `InvalidArgument` | None: assertion failure. Rust validated every argument and calls from the worker thread, so this status means a Lalela bug. |
| `FileNotFound` | `FileUnreadable` |
| `UnsupportedAudio` | `UnsupportedFormat` |
| `SpeechUnavailable` | `ProviderUnavailable { reason: "on-device speech is not supported on this Mac" }` |
| `LocaleUnsupported` | `UnsupportedLanguage` |
| `AssetsUnavailable` | `ModelUnavailable` |
| `Cancelled` | `Cancelled` |
| `TimedOut` | `Timeout` |
| `OutputTooLarge` | `MalformedResponse { detail: "shim output exceeded SHIM_SPEECH_OUTPUT_BYTES_MAX" }` |
| `NoPlayer` | None: assertion failure. The speech export never returns it. |
| `InsufficientResources` | `ProviderUnavailable { reason: "the system is busy with other speech analysis" }` |
| `Internal` | `PlatformFailure { status: 99 }` |
| `Unknown(n)` | `PlatformFailure { status: n }` |

Segment granularity is one finalised `SpeechTranscriber` result, typically a phrase or a sentence.

---

## Mistral Voxtral — `lalela-voxtral::VoxtralTranscriber`

| Item | Value |
|---|---|
| `ProviderId` | `MistralVoxtral` |
| Endpoint | `POST https://api.mistral.ai/v1/audio/transcriptions` |
| Auth | `Authorization: Bearer <key>` |
| Models | `voxtral-mini-2602` (default; hint: false), `voxtral-mini-transcribe-3-0` (hint: true), `voxtral-small-transcribe-3-0` (hint: true) |
| `audio_bytes_max` | `VOXTRAL_AUDIO_BYTES_MAX` |
| `audio_duration_ms_max` | `VOXTRAL_AUDIO_DURATION_MS_MAX` |
| Readiness | `NeedsApiKey` when no key; otherwise `Ready` |

Request: `multipart/form-data`, built by a hand-written streaming encoder in `voxtral/src/multipart.rs`. The boundary is `lalela-` followed by the 32 hex digits of `request.transcript_id`'s UUID. UUIDv7 carries 74 random bits, so a collision with bytes in the audio is negligible. Parts in order:

| Part | Value | Condition |
|---|---|---|
| `model` | `ModelOption.id` | Always |
| `timestamp_granularities` | `segment` | Always |
| `language` | Primary subtag of the tag, e.g. `fr` | `Tag` preference and `supports_language_hint` |
| `file` | File bytes, `filename` = `AudioSource.file_name`, `Content-Type` from `AudioFormat` | Always |

When the preference is `Tag` but the model does not support a language hint, the UI disables the language picker and shows "Auto"; the adapter asserts no `language` part is sent.

Response DTO (`text`, `language`, `segments[{text, start, end}]`, `model`, `usage`). Normalisation reads only `segments` and `language`. A 2xx response with no `segments` array is `MalformedResponse`. One with only empty segments is `NoSpeech`.

Progress: `Uploading` from the counting body reader, then `Processing { permille: None }` until the response arrives.

---

## Deepgram — `lalela-deepgram::DeepgramTranscriber`

| Item | Value |
|---|---|
| `ProviderId` | `Deepgram` |
| Endpoint | `POST https://api.deepgram.com/v1/listen` |
| Auth | `Authorization: Token <key>` |
| Models | `nova-3` (default; hint: true), `nova-2` (hint: true) |
| `audio_bytes_max` | `AUDIO_BYTES_MAX` (Deepgram accepts up to 2 GB) |
| `audio_duration_ms_max` | `AUDIO_DURATION_MS_MAX` |
| Readiness | `NeedsApiKey` when no key; otherwise `Ready` |

Request: the raw file bytes as the body, with `Content-Type` from `AudioFormat` and a known `Content-Length`. Query parameters:

| Parameter | Value | Condition |
|---|---|---|
| `model` | `ModelOption.id` | Always |
| `smart_format` | `true` | Always |
| `paragraphs` | `true` | Always |
| `language` | Full tag, e.g. `en-GB` | `Tag` preference |
| `detect_language` | `true` | `Auto` preference |

Normalisation: each `results.channels[0].alternatives[0].paragraphs.paragraphs[].sentences[]` entry becomes one `Segment` (`text`, `start`, `end`). `TranscriptDraft.language` comes from `results.channels[0].detected_language` when present, otherwise from the requested tag. A response without `paragraphs` is `MalformedResponse`.

Deepgram ends server processing at 10 minutes with HTTP 504, which maps to `Timeout` and is not retried.

---

## Shared HTTP behaviour — `lalela-http`

| Behaviour | Rule |
|---|---|
| TLS | `ureq` default `rustls` with bundled web PKI roots |
| Timeouts | `HTTP_TIMEOUT_CONNECT_MS`, `HTTP_TIMEOUT_SEND_BODY_MS`, `HTTP_TIMEOUT_RECEIVE_MS`, `HTTP_TIMEOUT_GLOBAL_MS` |
| Response cap | Body read with limit `HTTP_RESPONSE_BYTES_MAX`; exceeding it is `MalformedResponse` |
| Retries | At most `HTTP_RETRY_ATTEMPTS_MAX` extra attempts on connect failure, 429, 502, 503. Backoff from `HTTP_RETRY_BACKOFF_MS`, or `Retry-After` when ≤ `HTTP_RETRY_AFTER_MS_MAX`. No retry on 504 or any other status. |
| Cancellation | The upload reader returns an I/O error when `is_cancelled()` turns true, which aborts the send. Retry sleeps poll cancellation every `HTTP_CANCEL_POLL_MS`. A response wait cannot be interrupted, so the job ends when the response or the receive timeout arrives. |
| Redaction | `HttpError` holds status, provider request ID, and a truncated body excerpt. It never holds headers. |

---

## Implementation layout

```
crates/lalela-core/src/ports/transcriber.rs   port, descriptor, request, progress, error
crates/lalela-core/src/registry.rs            ProviderRegistry
crates/lalela-http/src/{client,retry,upload}.rs
crates/lalela-voxtral/src/{lib,descriptor,multipart,request,dto,decode}.rs
crates/lalela-voxtral/tests/fixtures/*.json   recorded responses (2602 and 3-0)
crates/lalela-deepgram/src/{lib,descriptor,request,dto,decode}.rs
crates/lalela-deepgram/tests/fixtures/*.json
crates/lalela-macos/src/speech.rs             AppleSpeechTranscriber
```

---

## Assumptions and open questions

**Assumptions**

- Mistral's transcription endpoint accepts `timestamp_granularities=segment` for every listed model, and Voxtral 3 returns segment timestamps only (word timestamps were removed in Voxtral 3).
- For `voxtral-mini-2602`, Mistral documents `timestamp_granularities` as incompatible with `language`. Lalela needs segments, so that model runs with automatic language detection only.
- Deepgram `nova-3` with `paragraphs=true` returns sentence-level `start`/`end` for pre-recorded audio.
- `SpeechTranscriber` produces final results with `audioTimeRange` attributes when it is configured without volatile results.

**Decisions**

- *Voxtral default model.* **`voxtral-mini-2602`.** It is generally available and documented for audio up to about three hours. The Voxtral 3 models became public preview on 2026-10-20 and are selectable, not default.
- *Deepgram segment unit.* **Sentences from `paragraphs`.** Sentences give finer highlighting than paragraphs and need no word-to-sentence grouping in Lalela. Utterances depend on pause detection and can run long.
- *Multipart encoding.* **Hand-written streaming encoder.** `ureq`'s multipart support is behind its `unversioned` module, outside semver. The encoder needs only text parts and one file part.
- *Deepgram body.* **Raw bytes, not a URL.** The audio is a local file, and a URL would require hosting it.
- *Retry set.* **Connect failures, 429, 502, 503 only.** These are transient. 504 from Deepgram means the job exceeded the processing window and would time out again.
- *Registry sharing.* **One `Arc<ProviderRegistry>` used by both threads.** The controller needs descriptors and readiness each frame, and the worker needs `transcribe`. Stateless adapters make `Send + Sync` free, which is cheaper than mirroring descriptors in a second structure.
- *Language hint plumbing.* **`supports_language_hint` per model.** It encodes the Voxtral 2 incompatibility in data, so the UI disables the picker instead of the request failing.

**Open questions**

- *Voxtral upload limits.* Mistral's limits page does not state the maximum direct-upload size or duration for Voxtral 3. `VOXTRAL_AUDIO_BYTES_MAX` and `VOXTRAL_AUDIO_DURATION_MS_MAX` hold conservative values until confirmed. Should larger files go through the Files API (`file_id`, up to 512 MB)?
- *Voxtral default after GA.* When Voxtral 3 leaves preview, should `voxtral-mini-transcribe-3-0` become the default?
- *Apple authorisation.* Apple's documentation does not say whether `SpeechAnalyzer` file transcription triggers a speech-recognition permission prompt. Does the bundle need `SFSpeechRecognizer.requestAuthorization` before the first call? A spike on macOS 27 is needed.
- *Apple asset concurrency.* macOS 27 adds an `insufficientResources` error when analysers exceed a default concurrency. Lalela runs one at a time; does another app's analyser count against it?
