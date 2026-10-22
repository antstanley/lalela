# Lalela — Design Overview

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io

**Lalela** (isiZulu and isiXhosa for "listen") is a small macOS desktop app. It transcribes an audio file with a provider the user picks, plays the audio back while highlighting the transcript, and saves the audio and its transcript side by side in a folder the user chooses. It is a Rust application with an egui interface and a hexagonal core.

This document is the entry point for Lalela's design. It states the problem, the goals, the system shape, and the scope. Detail pages are linked from each section.

---

## Problem

Transcribing a recording is easy to start and tedious to finish. Cloud transcription services return JSON that someone still has to check against the audio, and on-device transcription on macOS has no simple file-in, text-out tool. Checking a transcript means jumping between a media player and a text editor.

The best transcription engine depends on the recording. On-device transcription is private and free. Cloud models are often more accurate on hard audio, and they differ from each other in price and language coverage. A user should be able to switch engines without switching tools.

---

## Goals

1. Open an audio file by dialog or drag-and-drop, and transcribe it with one of three providers: Apple on-device Speech (`SpeechAnalyzer`), Mistral Voxtral, or Deepgram.
2. Let the user choose provider, model, and language (or automatic detection) before transcribing, and show progress with the option to cancel.
3. Play the audio with play/pause, scrubbing, and keyboard seeking, while the segment being spoken is highlighted. Clicking a segment jumps the audio to it.
4. Save to a user-chosen output folder as three files side by side: a copy of the audio, a plain-text transcript (`.txt`), and a timed sidecar (`.transcript.json`).
5. Reopen a saved audio file or sidecar and get the same synchronised playback without transcribing again.
6. Add a provider by adding one adapter crate that implements the `Transcriber` port, plus one enum variant and one registry line.

## Non-goals

- Live or microphone transcription. Lalela transcribes files only.
- Editing transcripts. The transcript is read-only; the user can edit the saved `.txt` in another tool.
- Speaker diarisation, translation, or summarisation.
- Batch transcription of many files, or a library browser for saved transcripts.
- Subtitle export (SRT, WebVTT).
- Platforms other than macOS 27+ on Apple silicon, and Mac App Store distribution.

---

## System shape

```
 ┌──────────────────────────── Lalela.app (one window) ──────────────────────────┐
 │                                                                               │
 │  Toolbar: Open · Provider · Model · Language · Transcribe/Cancel · Save · ⚙   │
 │  Transcript pane: timed segments, current one highlighted, click to seek     │
 │  Player bar: play/pause · scrubber · time · status line                       │
 │                                                                               │
 │        │ UiIntent                                   ▲ state, progress         │
 │        ▼                                            │                         │
 │  SessionController (main thread) ── Job ──▶ Worker thread                     │
 │        │          ▲                          │                                │
 │        │          └──── JobOutcome ──────────┤                                │
 │        ▼                                     ▼                                │
 │  AvAudioPlayer (shim)          Transcriber (one of three)   FsTranscriptStore │
 └───────────────────────────────────────┬───────────────────────────┬───────────┘
                                         │                           │
          ┌──────────────────────────────┼─────────────────┐         ▼
          ▼                              ▼                 ▼   <folder>/
   Apple Speech (on-device,      api.mistral.ai     api.deepgram.com  name.m4a
   via Swift shim)               /v1/audio/         /v1/listen        name.txt
                                 transcriptions                       name.transcript.json
```

- The **window** follows [design-guidelines.md](design-guidelines.md). The views turn user actions into `UiIntent` values and read state back. See [06-app-shell.md](06-app-shell.md).
- The **SessionController** owns the session state machine from [01-domain-model.md](01-domain-model.md). It drives the player directly on the main thread and hands long work to the worker.
- The **worker** runs one transcription or save at a time.
- The **Transcriber** port has three adapters, described in [02-transcription-providers.md](02-transcription-providers.md). Apple Speech and AVFoundation playback are reached through a Swift shim, described in [03-macos-shim.md](03-macos-shim.md).
- **Playback and highlighting** are in [04-playback.md](04-playback.md). **Saving and reopening** are in [05-persistence.md](05-persistence.md).

---

## Detail pages

| Page | Topic |
|---|---|
| [architecture-principles.md](architecture-principles.md) | Hexagonal layering, crates, dependency graph, threading |
| [development-guidelines.md](development-guidelines.md) | Tiger Style, Rust and Swift conventions, testing, jj workflow |
| [design-guidelines.md](design-guidelines.md) | Visual and interaction discipline, accessibility, design definition of done |
| [01-domain-model.md](01-domain-model.md) | Entities, IDs, invariants, session state machine |
| [02-transcription-providers.md](02-transcription-providers.md) | `Transcriber` port, registry, Apple / Voxtral / Deepgram adapters |
| [03-macos-shim.md](03-macos-shim.md) | Swift shim C ABI for Speech, AVFoundation, and appearance |
| [04-playback.md](04-playback.md) | `AudioPlayer` port, playhead polling, segment highlighting |
| [05-persistence.md](05-persistence.md) | Save layout, sidecar format, reopen, settings, Keychain |
| [06-app-shell.md](06-app-shell.md) | Window regions, intents, controller, worker, dialogs |
| [07-build-and-packaging.md](07-build-and-packaging.md) | Toolchain pinning, shim build, macOS 27 bundle |
| [08-limits.md](08-limits.md) | Every named limit constant and its value |
| [canonical-types.schema.json](canonical-types.schema.json) | Sidecar, settings, and domain shapes as JSON Schema |

This repository is a single project, so all specs sit in one layer in `.specs/`. The unnumbered pages are the cross-cutting rules; the numbered pages describe the app.

---

## Scope summary

| Area | Implementation | Notes |
|---|---|---|
| Input | One audio file per session: mp3, m4a/mp4/aac, wav, aiff, caf, flac | Size and duration capped; see [08-limits.md](08-limits.md) |
| Apple Speech | `SpeechTranscriber` via Swift shim; on-device assets installed on demand | No API key; locale must be supported on the device |
| Mistral Voxtral | `POST /v1/audio/transcriptions`, segment timestamps | Default model `voxtral-mini-2602`; API key in Keychain |
| Deepgram | `POST /v1/listen`, `nova-3`, paragraphs → sentence segments | API key in Keychain |
| Playback | AVAudioPlayer via shim; play/pause/seek; segment highlight | One player at a time |
| Save | Audio copy + `.txt` + `.transcript.json` in output folder; atomic writes | Name collisions get a numeric suffix |
| Reopen | Open an audio file with a sibling sidecar, or the sidecar itself | No re-transcription needed |
| Settings | Output folder, default provider, models, language in JSON; API keys in Keychain | Env vars override keys |
| UI | One egui window; keyboard shortcuts; VoiceOver through AccessKit | See [design-guidelines.md](design-guidelines.md) |
| Distribution | `Lalela.app` for macOS 27+, Apple silicon | Signing and notarisation open |

---

## Assumptions and open questions

**Assumptions**

- **This spec set is written before any code exists.** Every page describes the target design for the first implementation. Until the code lands, every body claim is a divergence that a spec-versus-code review will report, and the pages stay `Draft`.
- The user has network access when using Voxtral or Deepgram, and holds their own API keys for both.
- The Mac supports on-device `SpeechTranscriber` (`SpeechTranscriber.isAvailable`) for the Apple provider.

**Decisions**

- *Product scope.* **One file, one transcript, one window.** The owner asked for a minimal UI. Batch, library, and editing features would each need their own screens.
- *Saved artefacts.* **Audio copy, `.txt`, and `.transcript.json`.** The owner chose it. The `.txt` serves other tools, and the sidecar keeps segment timings so a saved pair can be reopened with synchronised playback.
- *Reopen.* **Supported through the sidecar.** The owner chose it. It reuses the playback view with no extra screens.
- *Platform floor.* **macOS 27, Apple silicon only.** The owner set macOS 27 as the floor. macOS 27 runs only on Apple silicon, so no Intel build exists.
- *App name.* **Lalela.** It matches the repository name and means "listen".

**Open questions**

- *Bundle identifier.* The spec uses `io.senzo.lalela` for the bundle ID and the Keychain service. Is that the intended identifier?
- *Signing and distribution.* Will Lalela be signed with a Developer ID and notarised, or run as an unsigned local build? This affects Keychain prompts and Gatekeeper.
