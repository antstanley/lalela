# Lalela specs

Canonical design specifications for Lalela, a macOS 27+ egui app that transcribes audio files with pluggable providers. This is a single-project repository, so every spec sits in this one directory. The unnumbered pages hold the cross-cutting rules. The numbered pages describe the app in reading order.

All pages are `Draft`. They were written before any code exists and describe the target design.

## Cross-cutting specs

| Spec | Topic |
|---|---|
| [architecture-principles.md](architecture-principles.md) | Hexagonal layering, ports, workspace crates, dependency graph, threading |
| [development-guidelines.md](development-guidelines.md) | Tiger Style, Rust and Swift conventions, testing, jj workflow, definition of done |
| [design-guidelines.md](design-guidelines.md) | Visual and interaction rules, keyboard, accessibility, design definition of done |
| [canonical-types.schema.json](canonical-types.schema.json) | JSON Schema for the sidecar, settings, and shim output |

## App specs

| Spec | Topic |
|---|---|
| [00-overview.md](00-overview.md) | Problem, goals, system shape, scope (start here) |
| [01-domain-model.md](01-domain-model.md) | Entities, IDs, invariants, session state machine |
| [02-transcription-providers.md](02-transcription-providers.md) | `Transcriber` port and the Apple, Voxtral, and Deepgram adapters |
| [03-macos-shim.md](03-macos-shim.md) | Swift shim C ABI for Speech, AVFoundation, and appearance |
| [04-playback.md](04-playback.md) | `AudioPlayer` port, playhead polling, segment highlighting |
| [05-persistence.md](05-persistence.md) | Saved layout, sidecar, reopen, settings, Keychain |
| [06-app-shell.md](06-app-shell.md) | Window, intents, controller, worker |
| [07-build-and-packaging.md](07-build-and-packaging.md) | Toolchain, shim build, `.app` bundle |
| [08-limits.md](08-limits.md) | Every named limit and its value |

## Change specs

(None yet.) Proposed changes go in `changes/` as `YYYY-MM-DD-short_snake_case_title.md`.
