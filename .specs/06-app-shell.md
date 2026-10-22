# 06 — App Shell

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io

This page defines `lalela-app`: the eframe window, its regions, how views turn user actions into intents, the `SessionController` that applies them, and the worker thread. The visual and interaction rules are in [design-guidelines.md](design-guidelines.md). The state machine is in [01-domain-model.md](01-domain-model.md#session-state-machine).

---

## Responsibilities

1. Build every adapter and the provider registry at startup. This is the composition root.
2. Render one window: toolbar, transcript pane, player bar with status line, and the settings sheet.
3. Translate input (clicks, shortcuts, drops) into `UiIntent` values and apply them through `SessionController`.
4. Run long jobs on the worker thread and fold their outcomes back into the session state.

The shell does **not** own: domain rules, provider protocols, file formats, or save planning. Those live in the core and the adapters.

---

## Startup

`main.rs`:

1. Install the stderr logger.
2. Assert at runtime that the OS version is ≥ 27.0, as a backstop to `LSMinimumSystemVersion`.
3. Build adapters: `HttpClient`, the three transcribers, `AvAudioPlayer`, `FsTranscriptStore`, `JsonSettingsStore`, `KeychainSecretStore`, `MacAppearance`, `SystemClock`, `UuidV7IdSource`.
4. Build `ProviderRegistry`, which asserts every `ProviderId` is registered, and wrap it in an `Arc`. Wrap `FsTranscriptStore` in an `Arc` too. The controller and the worker each hold a clone of both.
5. Load `Settings`. Read the key source (`Environment`, `Keychain`, or none) for each provider that needs a key, and cache it in the controller.
6. Spawn the worker thread with its two bounded channels.
7. In the app's creation callback, set `ctx.options_mut(|o| o.zoom_with_keyboard = false)`. egui otherwise binds ⌘= / ⌘− / ⌘0 to global zoom, which would collide with the transcript text shortcuts.
8. `eframe::run_native("Lalela", options, ...)` with `ThemePreference::System`, a minimum inner size of `WINDOW_WIDTH_PT_MIN` × `WINDOW_HEIGHT_PT_MIN`, and the `wgpu` renderer.

---

## Window regions

```
egui::Panel::top("toolbar")                 Open · file name · Provider · Model · Language
                                            · Transcribe/Cancel · Save · ⚙
egui::Panel::bottom("player")               ▶/❚❚ · position · scrubber · duration
                                            status line (state, progress, errors, "Show in Finder")
egui::CentralPanel                          transcript pane (ScrollArea::show_rows) or empty/drop state
egui::Modal "settings"                      output folder · API keys
egui::Modal "confirm"                       discard / replace confirmations
```

The views are free functions in `lalela-app/src/ui/` (`toolbar.rs`, `transcript.rs`, `player_bar.rs`, `status.rs`, `settings.rs`, `confirm.rs`). Each takes `&ViewModel` and `&mut Vec<UiIntent>` and has no other side effects.

---

## Intents

```rust
pub enum UiIntent {
    OpenDialog,                     OpenPath(PathBuf),
    SelectProvider(ProviderId),     SelectModel(&'static str),   SelectLanguage(LanguagePreference),
    Transcribe,                     Cancel,
    Save,                           ShowInFinder,
    PlayPause,                      Seek(Milliseconds),           SeekBy(i64),
    SegmentNext,                    SegmentPrevious,              SegmentActivate(u32),
    TranscriptTextLarger,           TranscriptTextSmaller,        TranscriptTextReset,
    OpenSettings,                   CloseSettings,
    ChooseOutputFolder,             SetApiKey(ProviderId, String), RemoveApiKey(ProviderId),
    Confirm(ConfirmChoice),
}

pub enum ConfirmChoice { Proceed, Cancel }

enum PendingAction { Open(PathBuf), Transcribe, Save(SavePlan), Quit }   // held by the controller
```

Selecting a provider, model, or language in the toolbar also writes it to `Settings`, so the toolbar selection is the default for the next launch. The settings sheet has no separate default-provider control.

An action that needs confirmation stores a `PendingAction` and opens the confirm sheet. `Confirm(Proceed)` runs the pending action, and `Confirm(Cancel)` or Esc drops it.

The transcript text intents change the transcript font size in `TRANSCRIPT_FONT_PT_STEP` steps between `TRANSCRIPT_FONT_PT_MIN` and `TRANSCRIPT_FONT_PT_MAX`. Reset returns to `TRANSCRIPT_FONT_PT_DEFAULT`. Only the transcript pane's `FontId` changes; egui's global zoom factor stays at 1.

Intents per frame are bounded by `INTENTS_PER_FRAME_MAX`. Exceeding it is a programmer error and asserts. The controller applies them in order, after views render and before the frame ends.

### Keyboard shortcuts

`lalela-app/src/ui/shortcuts.rs` maps `egui::KeyboardShortcut` values to intents with the table in [design-guidelines.md](design-guidelines.md#keyboard). Space and arrow keys are ignored while a text field has focus.

### Drag and drop

Each frame reads `ctx.input(|i| i.raw.dropped_files.clone())`. In egui 0.37 these are `DroppedFileHandle` values. The first handle's `path()` becomes `UiIntent::OpenPath`. More than one file shows "Drop one file at a time." `hovered_files` drives the drop-hover state.

---

## SessionController

`lalela-app/src/controller.rs` owns `SessionState`, `Settings`, the `AvAudioPlayer`, `KeychainSecretStore`, `JsonSettingsStore`, `MacAppearance`, `UuidV7IdSource`, clones of the shared registry and transcript store, the cached `PlayheadState` and key sources, the pending confirmation, the worker handles, and the current `ProgressCell` and `CancelToken`.

```
apply(intent):
    precondition checks (state legal? readiness? limits?)  → status message on failure
    side effect on main thread (player, dialog, settings write)  or  send Job to worker
    SessionState::apply(event)                              → asserted Ok
poll():
    playhead = player.state()
    while let Ok(outcome) = events.try_recv():  fold outcome into state
    build ViewModel
```

- `rfd` dialogs run inside `apply` on the main thread, as `rfd` requires on macOS.
- Readiness and limits are checked before a job starts: missing API key, file over the provider's `audio_bytes_max`, or duration over `audio_duration_ms_max`. Failures become status messages. The worker's assertions are the second line of defence.
- `Transcribe` reads the API key from `SecretStore`, gets a `TranscriptId` from `IdSource`, takes the next `JobId`, and sends `Job::Transcribe` with an `OwnedTranscriptionRequest`.
- `Save` calls `TranscriptStore::list_names` and `plan_save` on the main thread. A plan with `replaces_existing` goes through the confirm sheet. The plan then travels in `Job::Save`, and a `StoreError::Conflict` result re-plans once.
- `Open` checks for a sibling sidecar, calls `classify_open_path`, reads the sidecar with `TranscriptStore::read_sidecar` when one is involved, then calls `player.load`. All of this happens on the main thread.
- Closing the window asks for confirmation, through eframe's close-request handling, whenever an unsaved transcript would be lost: in `Unsaved`, in `Saving`, or in `Transcribing` / `Cancelling` with `Prior::Unsaved`.

---

## Worker

`lalela-app/src/worker.rs`. One thread, named `lalela-worker`.

```rust
pub struct JobId(u64);   // starts at 1 for each app run, +1 per job; never reused within a run

struct OwnedTranscriptionRequest {      // owned form of TranscriptionRequest, sent across threads
    transcript_id: TranscriptId,
    audio: AudioSource,
    model: &'static ModelOption,
    language: LanguagePreference,
    api_key: Option<ApiKey>,
}

enum Job {
    Transcribe { job_id: JobId, provider: ProviderId, request: OwnedTranscriptionRequest,
                 progress: Arc<ProgressCell>, cancel: CancelToken },
    Save { job_id: JobId, plan: SavePlan, audio: AudioSource, transcript: Transcript,
           progress: Arc<ProgressCell>, cancel: CancelToken },
    Shutdown,
}

enum JobOutcome {
    Transcribed { job_id: JobId, result: Result<Transcript, TranscribeError> },
    Saved { job_id: JobId, result: Result<SavedPair, StoreError> },
}
```

- The worker holds its clones of the `Arc<ProviderRegistry>` and `Arc<dyn TranscriptStore>`, plus the `SystemClock`. It receives with a blocking `recv()`, runs the job, sends one outcome, and calls `ctx.request_repaint()` once per outcome.
- `Transcript::from_draft` runs on the worker with `clock.now_unix_ms()`, so a malformed draft becomes an error before it reaches the UI.
- The command channel has capacity `WORKER_COMMANDS_QUEUE_MAX` and the event channel `WORKER_EVENTS_QUEUE_MAX`. The controller sends with `try_send`. The state machine guarantees at most one job in flight, so a full channel is a programmer error and asserts.
- On app exit, the controller sets the current `CancelToken`, sends `Shutdown`, and joins the worker for at most `WORKER_SHUTDOWN_WAIT_MS`. A worker blocked on an HTTP response is abandoned when the process exits.

`ProgressCell` holds an `AtomicU8` phase, an `AtomicU64` numerator, and an `AtomicU64` denominator. The worker side implements `TranscriptionObserver`. The observer has no `egui::Context`, so progress updates do not trigger repaints themselves. While a job runs, the UI reads the cell each frame and calls `ctx.request_repaint_after(PROGRESS_REPAINT_INTERVAL_MS)`, which keeps the progress bar and elapsed time current.

### Supporting ports

Defined in `lalela-core/src/ports/` and implemented in `lalela-app/src/platform.rs` and `lalela-macos`:

```rust
pub trait Clock: Send + Sync { fn now_unix_ms(&self) -> u64; }
pub trait IdSource { fn next_transcript_id(&self) -> TranscriptId; }
pub trait Appearance { fn accent_rgba(&self) -> Result<[u8; 4], AppearanceError>; }
pub enum AppearanceError { Platform { status: i32 } }
```

- `SystemClock` reads `SystemTime::now()` and asserts the result is after the Unix epoch.
- `UuidV7IdSource` formats `uuid::Uuid::now_v7()` and passes it through `TranscriptId::try_new`.
- `MacAppearance` calls `lalela_appearance_accent_rgba` on the main thread. If it fails, the UI uses egui's default selection colour and logs the status.

---

## View model

`ViewModel` is rebuilt each frame from controller state. It is the only thing views read.

| Field | Source |
|---|---|
| `file_name`, `duration` | `AudioSource` |
| `providers` with `readiness`, `models`, selected values | Registry + `Settings` + cached key sources (refreshed at startup and after each key change; no Keychain read per frame) |
| `primary_action` (Open / Transcribe / Save / none) | `SessionState` |
| `segments`, `current_segment` | `Transcript` + `segment_at_position` |
| `playhead`, `is_playing` | Cached `PlayheadState` |
| `status` (enum: Idle, Progress, Unsaved, Saved, Error) | `SessionState` + last error + `ProgressCell` |
| `accent` | `MacAppearance`, read at startup, when `ctx.system_theme()` changes, and when the window regains focus |
| `transcript_font_pt` | Controller field, changed by the transcript text intents |

---

## Implementation layout

```
crates/lalela-app/src/
├── main.rs            startup and composition root
├── app.rs             eframe::App impl: fn ui(&mut self, ui, frame)
├── controller.rs      SessionController
├── worker.rs          Job, JobOutcome, worker loop
├── progress.rs        ProgressCell, CancelToken
├── view_model.rs
├── platform.rs        SystemClock, UuidV7IdSource
└── ui/                toolbar, transcript, player_bar, status, settings, confirm,
                       shortcuts, theme, messages
```

---

## Assumptions and open questions

**Assumptions**

- eframe 0.37's `App::ui(&mut self, ui, frame)` is the per-frame entry point. `App::update` no longer exists.
- `egui::Modal` exists in egui 0.37 and supports focus containment for the settings and confirmation sheets.

**Decisions**

- *Views are pure.* **Views emit intents; only the controller mutates.** egui code is hard to unit-test. Keeping logic out of it means controller tests with fake ports cover the behaviour.
- *Pre-checks in the controller, assertions in the worker.* **Both.** The user gets a clear message, and a controller bug still cannot send an invalid job, per the paired-assertion rule.
- *Save on the worker.* **Saves run as jobs.** Copying a large audio file on the main thread would freeze the UI and playback.
- *Accent refresh.* **Re-read on theme change and on window focus.** macOS sends no notification that egui exposes when the accent colour changes. Users change it in System Settings, so returning focus to Lalela is the moment it can have changed.
- *Transcript text size.* **Scale the transcript `FontId`, not egui's zoom factor.** The zoom factor scales the whole UI, but the reading need applies to the transcript only.
- *Abandon on exit.* **Bounded join, then exit.** A blocking HTTP read can last minutes, and quitting the app must not wait for it.

**Open questions**

- *Menu bar.* eframe does not provide a native macOS menu bar. Do ⌘O / ⌘S / ⌘, need native menu items (through the shim) for discoverability and VoiceOver, or are in-window shortcuts enough?
- *Window state.* Should window size and position persist between launches through eframe's `persistence` feature?
