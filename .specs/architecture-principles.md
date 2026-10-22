# Architecture Principles

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io · **Scope:** Repo-wide

This page defines **how** the Lalela code is organised: the hexagonal layering, the Cargo workspace, the allowed dependency directions, the threading model, and the cross-cutting conventions. It is the canonical reference for "core", "port", "adapter", and "composition root" everywhere in the repo.

The coding discipline these rules sit inside is [development-guidelines.md](development-guidelines.md). What the app does is in [00-overview.md](00-overview.md).

---

## Hexagonal layering

Lalela uses ports and adapters. The **core** holds the domain types, the rules, and the use cases. It knows nothing about egui, HTTP, Swift, the filesystem, or the Keychain. Every outside capability reaches the core through a **port**, which is a Rust trait defined in the core. An **adapter** is a crate that implements one or more ports against a real technology. The **composition root** (`lalela-app`) constructs the adapters, hands them to the core, and owns the UI.

```
                 ┌──────────────────────── lalela-app ────────────────────────┐
                 │  egui views ──UiIntent──▶ SessionController ──▶ Worker     │
                 │        (driving adapter)          │  composition root       │
                 └───────────────────────────────────┼─────────────────────────┘
                                                     ▼
          ┌───────────────────────────── lalela-core ─────────────────────────────┐
          │  domain types · invariants · use cases · SessionState machine         │
          │                                                                       │
          │  ports:  Transcriber   AudioPlayer   TranscriptStore   SettingsStore  │
          │          SecretStore   Clock         IdSource          Appearance     │
          └──────▲──────────▲────────────▲─────────────▲──────────────▲───────────┘
                 │          │            │             │              │
        lalela-voxtral  lalela-deepgram  lalela-macos  lalela-store   lalela-app
         (Transcriber)   (Transcriber)   (Transcriber, (Transcript-   (Clock,
                │              │          AudioPlayer,  Store,         IdSource)
                └──lalela-http─┘          SecretStore,  SettingsStore)
                                          Appearance)
                                              │
                                        shim/LalelaShim (Swift, C ABI)
```

### What goes where

| Concern | Layer |
|---|---|
| `Transcript`, `Segment`, `AudioSource`, IDs, units, validation | core |
| Session state machine and legal transitions | core |
| Current-segment lookup for a playback position | core |
| Save planning: file stems, collision suffixes, target names | core |
| Provider registry and readiness rules | core |
| HTTP transport, retries, response size caps | `lalela-http` |
| Voxtral and Deepgram request building and response decoding | `lalela-voxtral`, `lalela-deepgram` |
| Apple Speech transcription, AVFoundation playback, Keychain, system accent colour | `lalela-macos` (+ Swift shim) |
| Atomic file writes, sidecar and settings JSON encoding | `lalela-store` |
| egui views, the worker thread, file dialogs, wall clock, UUID generation | `lalela-app` |

### Ports

| Port | Purpose | Implemented by |
|---|---|---|
| `Transcriber` | Turn an audio file into a `TranscriptDraft`; report readiness and progress; observe cancellation | `VoxtralTranscriber`, `DeepgramTranscriber`, `AppleSpeechTranscriber` |
| `AudioPlayer` | Load, play, pause, seek, report position and duration | `AvAudioPlayer` |
| `TranscriptStore` | Execute a `SavePlan`; read a sidecar; list names in a folder | `FsTranscriptStore` |
| `SettingsStore` | Load and save `Settings` | `JsonSettingsStore` |
| `SecretStore` | Get, set, and delete an `ApiKey` per provider | `KeychainSecretStore` (env-var override layered on top) |
| `Clock` | Current time as Unix milliseconds | `SystemClock` |
| `IdSource` | New `TranscriptId` values | `UuidV7IdSource` |
| `Appearance` | System accent colour | `MacAppearance` |

Port contracts and their full signatures live on the pages that use them: `Transcriber` in [02-transcription-providers.md](02-transcription-providers.md), `AudioPlayer` in [04-playback.md](04-playback.md), `TranscriptStore`, `SettingsStore`, and `SecretStore` in [05-persistence.md](05-persistence.md), and `Clock`, `IdSource`, and `Appearance` in [06-app-shell.md](06-app-shell.md#supporting-ports).

---

## Workspace layout

```
lalela/
├── Cargo.toml                 workspace, [workspace.lints], [workspace.dependencies]
├── rust-toolchain.toml
├── rustfmt.toml               max_width = 100
├── .cargo/config.toml         target aarch64-apple-darwin; MACOSX_DEPLOYMENT_TARGET = 27.0
├── crates/
│   ├── lalela-core/           domain, ports, use cases, limits
│   ├── lalela-http/           blocking HTTP helper over ureq
│   ├── lalela-voxtral/        Mistral Voxtral Transcriber adapter
│   ├── lalela-deepgram/       Deepgram Transcriber adapter
│   ├── lalela-macos/          Swift shim build + FFI adapters
│   ├── lalela-store/          filesystem + JSON adapters
│   └── lalela-app/            eframe binary, composition root
├── shim/
│   ├── Package.swift          SwiftPM static library, platforms: macOS 27
│   ├── include/lalela_shim.h  the C ABI, single source of truth
│   └── Sources/LalelaShim/    Speech.swift, Player.swift, Appearance.swift, Status.swift
└── .specs/
```

### Crate roles

- **lalela-core** — Pure Rust. Holds domain types, invariants, ports, use cases, and the core limit constants (`limits.rs`). HTTP and shim limits live in the crates that apply them ([08-limits.md](08-limits.md)). It has no I/O and no threads. Its only dependency is `thiserror`.
- **lalela-http** — A `HttpClient` wrapper over `ureq` with fixed timeouts, a bounded retry loop, a response-size cap, and a counting/cancellable upload reader. It does not know about providers.
- **lalela-voxtral**, **lalela-deepgram** — One `Transcriber` each. Each crate has a request builder, a pure `decode_response`, and the adapter that joins them through `lalela-http`.
- **lalela-macos** — The only crate with `unsafe`. Its `build.rs` builds the Swift shim. It holds the FFI declarations, the `ShimStatus` mirror, and four adapters: `AppleSpeechTranscriber`, `AvAudioPlayer`, `KeychainSecretStore`, `MacAppearance`.
- **lalela-store** — `FsTranscriptStore` and `JsonSettingsStore`. It owns the serde DTOs for the sidecar and settings files. The core types do not derive `serde`.
- **lalela-app** — `main.rs`, the eframe `App`, egui views, `SessionController`, the worker thread, `SystemClock`, `UuidV7IdSource`, and the provider registry wiring.

---

## Dependency graph

```
lalela-app ──▶ lalela-core
    │
    ├──▶ lalela-voxtral ──┬──▶ lalela-http ──▶ ureq
    ├──▶ lalela-deepgram ─┘        │
    │          │                   └──▶ lalela-core
    │          └──▶ lalela-core
    ├──▶ lalela-macos ──▶ lalela-core, security-framework, LalelaShim (static)
    └──▶ lalela-store ──▶ lalela-core, serde, serde_json
```

Rules:

- Every arrow points toward `lalela-core`. The core depends on no workspace crate.
- Adapter crates never depend on each other, except that the two HTTP providers depend on `lalela-http`.
- Only `lalela-app` depends on `eframe`, `egui`, or `rfd`.
- Only `lalela-macos` links the Swift shim. Linking it from two crates would duplicate symbols.

### Third-party dependency allow-list

| Crate | Used by | Why |
|---|---|---|
| `thiserror` | all library crates | Error enum derivation with no runtime cost |
| `ureq` (3.x, `rustls`) | `lalela-http` | Blocking HTTP with no async runtime |
| `serde`, `serde_json` | `lalela-voxtral`, `lalela-deepgram`, `lalela-macos`, `lalela-store` | Wire, shim-output, and file DTOs |
| `security-framework` | `lalela-macos` | Keychain generic passwords |
| `eframe`, `egui` (0.37) | `lalela-app` | UI |
| `rfd` | `lalela-app` | Native open and folder dialogs |
| `uuid` (`v7`) | `lalela-app` | `TranscriptId` generation |
| `log` | adapters, `lalela-app` | Logging facade |
| `proptest` | dev-dependency | Property tests |

Adding a crate to this table is an architecture change and needs a Decision entry on this page.

---

## Threading model

Lalela runs exactly two long-lived threads.

| Thread | Owns | Never does |
|---|---|---|
| Main (UI) | eframe event loop, egui views, `SessionController`, `AvAudioPlayer`, `rfd` dialogs, `MacAppearance`, `KeychainSecretStore`, `JsonSettingsStore`, `UuidV7IdSource`; provider descriptors and readiness; capped store reads (`list_names`, `read_sidecar`) | Blocking network or speech calls, file copies |
| Worker (`lalela-worker`) | Running one `Job` at a time: a transcription (`Transcriber::transcribe`) or a save (`TranscriptStore::execute`); `SystemClock` | egui calls other than `Context::request_repaint`, player calls, dialogs, Keychain access |

- Commands go from main to the worker over a bounded `sync_channel` with capacity `WORKER_COMMANDS_QUEUE_MAX`. Completions come back over a bounded `sync_channel` with capacity `WORKER_EVENTS_QUEUE_MAX`.
- Progress is a shared atomic `ProgressCell`, not a stream of messages. The worker overwrites it and the UI reads it each frame, so progress needs no queue.
- Cancellation is a shared `AtomicBool` in the job's `CancelToken`.
- `ProviderRegistry` and `FsTranscriptStore` are shared between the threads as `Arc`s. Their traits are `Send + Sync`, and their adapters hold no mutable state.
- The worker calls `egui::Context::request_repaint()` once per job outcome. While a job runs, the UI schedules its own repaints every `PROGRESS_REPAINT_INTERVAL_MS` to show progress.
- The `SessionController` starts at most one job at a time (`JOBS_IN_FLIGHT_MAX = 1`). The session state machine makes a second start unrepresentable.

The full flow is in [06-app-shell.md](06-app-shell.md).

---

## Cross-cutting conventions

### Errors

- Each crate has one `Error` enum. Port traits return the port's own error type (`TranscribeError`, `PlayerError`, `StoreError`, `SecretError`), defined in the core.
- Adapters translate every vendor error into the port error with a `From` impl or an explicit `match`.
- The UI maps each port error to one sentence in the status line. The mapping lives in `lalela-app/src/ui/messages.rs`.

### IDs

- `TranscriptId` is the only persistent ID: `trn_` followed by a lowercase hyphenated UUIDv7. Its scheme is in [01-domain-model.md](01-domain-model.md).
- `ProviderId` is an enum. Its stable string forms (`apple-speech`, `mistral-voxtral`, `deepgram`) appear in files and Keychain account names.

### Time

- Wall-clock time enters only through the `Clock` port as `u64` Unix milliseconds.
- Media time is `Milliseconds(u64)` from the start of the audio. Providers that report seconds as floats are converted once, in the adapter, by rounding to the nearest millisecond.

### Logging

- Adapters and `lalela-app` log through the `log` facade. `lalela-app` installs a stderr logger.
- `lalela-core` does not log. It returns values, and callers decide what to record.
- Logs never contain API keys, request headers, or transcript text. They may contain file names, durations, provider ids, HTTP status codes, and provider request IDs.

---

## Platform baseline

| Item | Value |
|---|---|
| OS | macOS 27.0 or later |
| CPU | Apple silicon (`aarch64-apple-darwin`) |
| Rust | stable ≥ 1.95 |
| Swift | ≥ 6.3 (Xcode 27) |
| UI | eframe / egui 0.37, `wgpu` renderer |

Build and packaging details are in [07-build-and-packaging.md](07-build-and-packaging.md).

---

## Assumptions and open questions

**Assumptions**

- The repository is greenfield. This page is the target architecture; no crate exists yet.
- eframe on macOS runs the egui event loop on the process main thread, which AppKit and AVFoundation calls from the shim require.
- `egui::Context::request_repaint()` is safe to call from the worker thread (documented for eframe).

**Decisions**

- *Architecture.* **Hexagonal, with ports defined in the core.** Three interchangeable transcription back ends are the central requirement. A port keeps each one replaceable and testable with fakes.
- *Async runtime.* **None; one blocking worker thread.** Lalela runs one job at a time. `ureq` and a semaphore-blocked shim call keep control flow linear and avoid a Tokio dependency.
- *Progress transport.* **Atomic cell, not a channel.** Progress is state, so the latest value is all the UI needs. A bounded progress channel would have to either block the worker or drop messages.
- *Provider identity.* **`ProviderId` is a closed enum.** Adding a provider already needs a new crate and registry wiring; one enum variant more makes every `match` show the new case, which a string ID would hide.
- *Serde boundary.* **DTOs in adapters, no `serde` derive on core types.** File and wire formats change for reasons unrelated to domain rules, and every decode passes through the core's validating constructors.
- *Shared HTTP crate.* **`lalela-http`.** Retry, timeout, and size-cap logic is identical for both HTTP providers. Duplicating it would let the two drift.
- *Playback back end.* **AVFoundation through the shim, not rodio.** The shim is required for Apple Speech anyway. AVFoundation decodes every format macOS supports, including HE-AAC and ALAC, which symphonia does not fully cover.

**Open questions**

- *System logging.* Should `lalela-app` log to the macOS unified log (`os_log` through the shim) instead of stderr, so that logs from a bundled app are visible in Console.app?
