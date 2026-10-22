# 03 — macOS Shim

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io

This page defines `LalelaShim`, the Swift static library that gives Rust access to Apple frameworks that have no C or Objective-C API, and `lalela-macos`, the Rust crate that wraps it. The shim exposes three areas: Speech (`SpeechAnalyzer`), playback (`AVAudioPlayer`), and appearance (`NSColor.controlAccentColor`). The ports these back are on [02-transcription-providers.md](02-transcription-providers.md) and [04-playback.md](04-playback.md). The Swift coding rules are in [development-guidelines.md](development-guidelines.md#swift-conventions).

---

## Responsibilities

1. Expose a small, synchronous C ABI over Swift-only, `async` Apple APIs.
2. Convert Apple results into plain data: status codes, integers, and a JSON byte buffer.
3. Keep Apple objects (analyser, player) alive between calls inside the shim, behind a fixed-size slot.
4. Never let a Swift error, exception, or unwinding panic cross the boundary.

The shim does **not** own: domain validation (Rust re-validates every value), limits policy (Rust passes capacities), threading decisions, or any UI.

---

## ABI

`shim/include/lalela_shim.h` is the single source of truth. `crates/lalela-macos/src/ffi.rs` mirrors it by hand, and a unit test asserts that the `ShimStatus` discriminants match.

```c
#include <stdbool.h>
#include <stdint.h>

typedef int32_t lalela_status;          /* values below */

typedef void (*lalela_progress_fn)(void *context, uint32_t phase, uint32_t permille);
typedef bool (*lalela_cancelled_fn)(void *context);

/* Speech. is_available: any thread. transcribe: worker thread only (never main). */
bool          lalela_speech_is_available(void);
lalela_status lalela_speech_transcribe(
    const char *audio_path,            /* UTF-8, NUL-terminated, absolute */
    const char *locale_identifier,     /* UTF-8; "" = system locale */
    uint64_t timeout_ms,
    void *context,
    lalela_progress_fn on_progress,
    lalela_cancelled_fn is_cancelled,
    uint8_t *out_json,
    uint32_t out_json_capacity,
    uint32_t *out_json_length);

/* Player — main thread only. One player slot. */
lalela_status lalela_player_open(const char *audio_path, uint64_t *out_duration_ms);
lalela_status lalela_player_play(void);
lalela_status lalela_player_pause(void);
lalela_status lalela_player_seek(uint64_t position_ms);
lalela_status lalela_player_state(uint64_t *out_position_ms, bool *out_is_playing);
void          lalela_player_close(void);

/* Appearance — main thread only. */
lalela_status lalela_appearance_accent_rgba(uint8_t out_rgba[4]);
```

### Status codes

| Value | Name | Meaning |
|---|---|---|
| 0 | `Ok` | Success |
| 1 | `InvalidArgument` | Null pointer, zero capacity, non-UTF-8 path, or wrong thread |
| 2 | `FileNotFound` | Path does not exist or is unreadable |
| 3 | `UnsupportedAudio` | AVFoundation cannot decode the file |
| 4 | `SpeechUnavailable` | `SpeechTranscriber.isAvailable` is false |
| 5 | `LocaleUnsupported` | No supported locale is equivalent to the request |
| 6 | `AssetsUnavailable` | On-device assets could not be installed |
| 7 | `Cancelled` | `is_cancelled` returned true and analysis stopped |
| 8 | `TimedOut` | `timeout_ms` elapsed |
| 9 | `OutputTooLarge` | Result JSON exceeds `out_json_capacity` |
| 10 | `NoPlayer` | Player call with no open player |
| 11 | `InsufficientResources` | `SpeechAnalyzer` refused for concurrency |
| 99 | `Internal` | Any other thrown error; the shim logs it with `os_log` |

Rust maps any value not in this table to `ShimStatus::Unknown(i32)`, which becomes `TranscribeError::PlatformFailure` or `PlayerError::Platform`. It does not trust the raw value. The full speech mapping is in [02-transcription-providers.md](02-transcription-providers.md#apple-on-device-speech--lalela-macosapplespeechtranscriber).

### Progress phases

| `phase` | Meaning | `permille` |
|---|---|---|
| 1 | Installing on-device assets | `Progress.fractionCompleted × 1000` |
| 2 | Analysing | last final result's end time ÷ file duration × 1000 |

---

## Speech transcription flow

`lalela_speech_transcribe` runs on the calling (worker) thread and blocks it until finished.

```
validate args (non-null, capacity > 0, not main thread)
call = CallState(context, on_progress, is_cancelled, out_json, capacity)   // lock, abandoned flag, status
DispatchSemaphore(0); task = Task { do {
  guard SpeechTranscriber.isAvailable                         → SpeechUnavailable
  locale = await SpeechTranscriber.supportedLocale(equivalentTo:
             identifier.isEmpty ? .current : Locale(identifier:)) → LocaleUnsupported
  transcriber = SpeechTranscriber(locale:, transcriptionOptions: [],
                  reportingOptions: [], attributeOptions: [.audioTimeRange])
  if let req = try await AssetInventory.assetInstallationRequest(supporting: [transcriber]) {
      observe req.progress → call.progress(1, ‰); try await req.downloadAndInstall()
  }                                                           → AssetsUnavailable
  file = try AVAudioFile(forReading: url)                     → FileNotFound / UnsupportedAudio
  analyzer = SpeechAnalyzer(modules: [transcriber])
  collector = Task { for try await r in transcriber.results where r.isFinal { append } }
  watchdog  = Task { every SPEECH_CANCEL_POLL_MS: if call.cancelled() { await analyzer.cancelAndFinishNow() } }
  last = try await analyzer.analyzeSequence(from: file)
  if let last { try await analyzer.finalizeAndFinish(through: last) }
  else        { await analyzer.cancelAndFinishNow() }
  await collector; watchdog.cancel()
  encode {locale.identifier(.bcp47), segments}; call.write(json) → OutputTooLarge
  call.status = Ok
} catch { call.status = map(error) }                          // any throw above
  watchdog?.cancel(); progressObservation?.invalidate()
  semaphore.signal()
}
timedOut = semaphore.wait(timeout: timeout_ms) == .timedOut
call.abandon()   // EVERY exit path: takes the lock, sets abandoned, waits out any in-flight callback
task.cancel()    // cooperative; anything still running can no longer reach Rust memory
return timedOut ? TimedOut : call.status
```

- Each final result becomes one segment. `start_ms` and `end_ms` come from `result.range` (`CMTimeRange`), and `text` from the result's characters.
- `is_cancelled` and `on_progress` are called from Swift tasks on other threads. The Rust side backs both with atomics in `ProgressCell` and `CancelToken`, which are `Sync`.
- **No Rust memory is touched after the export returns.** `CallState` is a Swift class holding the Rust `context`, both callbacks, and the output buffer pointer, guarded by an `NSLock` and an `abandoned` flag. `call.progress`, `call.cancelled`, and `call.write` each take the lock, return without touching any Rust pointer if `abandoned` is set, and otherwise make the call or the copy while still holding the lock. Before every return (success, error, or timeout), `call.abandon()` takes the same lock, so it waits for any in-flight callback to finish. This covers the watchdog Task and the asset-progress observer, which may still be between a sleep and a callback when the export finishes. A Task that outlives the call sees `abandoned` and touches nothing. `call.cancelled` returns `true` once abandoned, so the orphaned Task stops at its next poll.
- The locale is written with `Locale.identifier(.bcp47)` (`en-US`), not `Locale.identifier` (`en_US`), so it validates as a `LanguageTag`.
- The analyser is created per call and released before return. No Speech state survives between calls.

The output JSON (`ShimSpeechOutput` in [canonical-types.schema.json](canonical-types.schema.json)) is written to `out_json`. It is UTF-8 and not NUL-terminated:

```json
{ "locale": "en-US", "segments": [ { "start_ms": 0, "end_ms": 2140, "text": "Welcome back." } ] }
```

---

## Player

The shim holds a single `AVAudioPlayer?` slot, which is `PLAYERS_OPEN_MAX = 1`.

| Call | Behaviour |
|---|---|
| `lalela_player_open` | Closes any open player, creates `AVAudioPlayer(contentsOf:)`, calls `prepareToPlay()`, and writes `duration × 1000` rounded. |
| `lalela_player_play` / `pause` | Calls `play()` / `pause()`. `NoPlayer` if the slot is empty. |
| `lalela_player_seek` | Sets `currentTime` to `position_ms / 1000`, clamped to `[0, duration]`. Keeps the play/pause state. |
| `lalela_player_state` | Writes `currentTime × 1000` rounded and `isPlaying`. |
| `lalela_player_close` | Stops and releases the player. Idempotent. |

`AvAudioPlayer` maps statuses to `PlayerError`: `NoPlayer` → `NotLoaded`, `FileNotFound` → `FileNotFound`, `UnsupportedAudio` → `UnsupportedAudio`, `InvalidArgument` → assertion failure (wrong thread or bad pointer is a Lalela bug), anything else → `Platform { status }`.

Every player call checks `Thread.isMainThread` and returns `InvalidArgument` otherwise.

---

## Rust wrapper — `lalela-macos`

```
crates/lalela-macos/
├── build.rs              builds shim/ with SwiftPM, emits link flags (see 07)
├── src/ffi.rs            extern "C" declarations, ShimStatus
├── src/speech.rs         AppleSpeechTranscriber: Transcriber
├── src/player.rs         AvAudioPlayer: AudioPlayer
├── src/appearance.rs     MacAppearance: Appearance
├── src/keychain.rs       KeychainSecretStore: SecretStore (security-framework, no shim)
└── tests/live_shim.rs    #[ignore] tests against fixture audio
```

FFI rules:

- Each `unsafe` call sits in a function of at most 30 lines with a `// SAFETY:` comment naming the pointer validity and lifetime it relies on.
- Paths are passed as `CString` built from the UTF-8 path. A path containing NUL is rejected before the call.
- The callbacks are `extern "C"` functions. Each body is wrapped in `std::panic::catch_unwind`, and on a caught panic the process aborts, so no unwind crosses into Swift.
- `context` is a `*const SpeechCallContext` borrowed for the duration of the call. The shim's `CallState` lock guarantees no callback runs after the call returns, including after `TimedOut`, so the borrow outlives every callback.
- After a successful call, Rust asserts `out_json_length ≤ out_json_capacity` before slicing.
- `AvAudioPlayer` is `!Send`, using a `PhantomData<*const ()>` marker, so the type system keeps it on the main thread.

---

## Assumptions and open questions

**Assumptions**

- `SpeechAnalyzer`, `SpeechTranscriber`, and `AssetInventory` are Swift-only and have no Objective-C or C surface. The macOS 27 SDK headers expose only the legacy `SF*` classes to Objective-C.
- `analyzeSequence(from: AVAudioFile)` (macOS 26) remains available on macOS 27.
- AVAudioPlayer reports `currentTime` with millisecond-level precision during playback.

**Decisions**

- *Bridge mechanism.* **Plain SwiftPM static library with `@c` exports, built from `build.rs`.** `swift-bridge` is still pre-1.0 and code-generation heavy. `swift-rs` adds a dependency to do what a short `build.rs` does.
- *Async to sync.* **Semaphore-blocked worker thread.** Rust has no async runtime. Blocking the dedicated worker is the simplest correct bridge, and the main thread never calls the blocking export.
- *Result transport.* **JSON in a caller-owned buffer.** No heap ownership crosses the ABI, and Rust decodes through the same validating path as the HTTP providers.
- *Input API.* **`AVAudioFile` with `analyzeSequence(from:)`, not the macOS 27 `AssetInputSequenceProvider`.** The older API is enough for local files and keeps the shim smaller. The newer provider adds format conversion that `AVAudioFile` already handles.
- *Timeout safety.* **Lock-guarded `abandoned` flag in the shim, not a bounded wait for the Task.** Swift Task cancellation is cooperative, and `SpeechAnalyzer` may not stop promptly. Waiting again would only move the timeout. The flag makes the export's return a hard boundary after which Rust memory is never touched.
- *Single player slot.* **One global player in the shim.** A session has one audio file, and a handle table would add lifetime rules that nothing needs.

**Open questions**

- *Speech timeout.* What value should `SPEECH_TIMEOUT_MS` take for a three-hour file on the slowest supported Mac? It needs measurement.
- *Volatile results.* Would requesting volatile results let the UI show text while a long file is still analysing, and is that worth adding a streaming callback to the ABI?
