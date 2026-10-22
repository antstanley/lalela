# Design Guidelines

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io · **Scope:** Repo-wide

This page governs Lalela's only visual surface: a single egui/eframe window on macOS 27. It records the adopted visual and interaction discipline. The UI is not implemented yet. Every rule below is adopted policy, and no statement here claims existing tokens, renders, or checks.

Screen-specific layout and flows live in [06-app-shell.md](06-app-shell.md). The named constants cited below are listed with their values in [08-limits.md](08-limits.md). Coding rules live in [development-guidelines.md](development-guidelines.md).

---

## Context and authority

Lalela is for one person working at a Mac. They open a recording, transcribe it, check the text against the audio, and save both. The work happens in short sessions. Most time is spent reading the transcript while the audio plays.

| Source | Authority | Evidence scope |
|---|---|---|
| This page | Adopted design policy | Policy only; no UI exists |
| macOS system appearance (light/dark, accent colour) | Runtime colour inputs | Read at runtime; not inspected |
| egui `Visuals::light()` / `Visuals::dark()` | Base widget styling that Lalela overrides | egui 0.37 defaults; not inspected in a render |

There is no brand document, `DESIGN.md`, or token file. The theme lives in code as one module (`lalela-app/src/ui/theme.rs`). It becomes the implementation authority once it exists.

---

## Design principles

| Adopted principle | Consequence for design |
|---|---|
| The transcript is the content | The transcript pane takes all space not used by the toolbar and player bar. Controls never overlay the text. |
| Quiet, macOS-leaning chrome | Lalela follows the system appearance and accent colour. It has no brand colour, decorative imagery, or ambient animation. |
| One obvious next action | In each session state, at most one toolbar button uses the accent fill: Open, Transcribe, or Save. |
| State is always visible | The status line always names the current state: idle, transcribing (with progress), cancelling, unsaved, saving (with progress), saved (with path), or an error. |
| Stable layout | Toolbar, transcript, and player bar keep fixed positions. Disabled controls stay in place rather than disappearing. |

---

## Visual foundations

| Foundation | Adopted usage | Source or implementation evidence |
|---|---|---|
| Theme | Follows macOS light/dark appearance live. eframe runs with `ThemePreference::System`. | Policy; unimplemented |
| Accent colour | The macOS accent colour (`NSColor.controlAccentColor`, read through the shim) marks the primary button, the current transcript segment, the scrubber fill, and keyboard focus. | Policy; unimplemented |
| Semantic colour | Only four roles beyond egui defaults: `accent`, `accent_subtle` (current-segment background, accent at low alpha), `error`, `text_muted` (timestamps, status line). | Policy; unimplemented |
| Typography | The macOS system font, loaded at runtime from the system font directory, with egui's bundled font as fallback. Transcript body is `TRANSCRIPT_FONT_PT_DEFAULT` (15 pt) with 1.4 line spacing. Controls are 13 pt. Timestamps use egui's monospace font at 12 pt, tabular. | Policy; unimplemented |
| Layout | Three fixed regions. The top panel is the toolbar. The central panel is a scroll area with the transcript. The bottom panel holds the player bar and the status line. Transcript text wraps at the pane width, with a maximum line length of `TRANSCRIPT_LINE_CHARS_MAX` (80) characters for readability. | Policy; unimplemented |
| Density | macOS-like spacing: 8 pt base unit, 6 pt item spacing, 28 pt minimum control height. | Policy; unimplemented |
| Shape | egui default corner radius. No shadows except egui's default modal shadow on the settings sheet. | Policy; unimplemented |
| Iconography | Text labels on every toolbar control. The player uses ▶ / ❚❚ glyphs from the bundled font with accessible names. No icon font or image assets. | Policy; unimplemented |

---

## Interaction conventions

### Keyboard

Every action is reachable by keyboard. Shortcuts follow macOS conventions.

| Shortcut | Action | Available when |
|---|---|---|
| ⌘O | Open audio or saved transcript | No job running (not transcribing, cancelling, or saving) |
| ⌘↩ | Transcribe | Audio loaded, no job running |
| ⌘. | Cancel transcription | Transcribing |
| ⌘S | Save to output folder | Unsaved transcript present |
| ⌘, | Open settings | Always |
| Space | Play / pause | Audio loaded, focus not in a text field |
| ← / → | Seek back / forward `SEEK_STEP_MS` (5 s) | Audio loaded, focus not in a text field |
| ⌥← / ⌥→ | Previous / next segment | Transcript present |
| ⌘= / ⌘− | Larger / smaller transcript text | Always |
| ⌘0 | Reset transcript text size | Always |
| Esc | Close settings or confirmation dialog | Dialog open |

Tab order runs toolbar (left to right), then transcript segments (top to bottom), then player bar. Focus is always visible as a 2 pt accent-coloured stroke.

### States

| State or pattern | Expected behaviour | Applicable surfaces |
|---|---|---|
| Empty (no audio) | The transcript pane shows one centred line: "Open or drop an audio file." The player bar is disabled. | Transcript pane, player bar |
| Drop hover | While a file hovers, the transcript pane shows an accent-stroked outline and "Drop to open." | Transcript pane |
| Audio loaded, no transcript | The transcript pane says "Ready to transcribe with <Provider>." Playback works. | Transcript pane |
| Transcribing | The Transcribe button becomes Cancel. A determinate progress bar shows in the status line when the provider reports progress, otherwise an indeterminate spinner with elapsed time. Open, Save, and the provider, model, and language pickers are disabled. A transcript from before this run stays visible and playable. | Toolbar, status line, transcript pane |
| Cancelling | Cancel is disabled and reads "Cancelling…". The status line says "Stopping <Provider>…" until the worker finishes. The prior transcript, if any, comes back with no error message. | Toolbar, status line |
| Saving | Save is disabled. The status line shows a determinate bar for the audio copy, then "Saved to <folder>". Open and Transcribe are disabled. Playback keeps working. | Toolbar, status line |
| Transcript present | Segments list with timestamp and text. During playback the current segment has the `accent_subtle` background and a 3 pt accent bar on its leading edge. The view auto-scrolls to keep it visible unless the user scrolled within `AUTOSCROLL_SUPPRESS_MS` (3 s). | Transcript pane |
| Segment activation | Clicking a segment, or pressing Return on a focused one, seeks to its start and keeps the play/pause state. | Transcript pane |
| Unsaved | The status line reads "Unsaved." Closing the window, opening another file, or transcribing again asks for confirmation. | Status line, dialogs |
| Saved | The status line reads "Saved to <folder>" with a "Show in Finder" link. | Status line |
| Error | The status line shows the error in `error` colour with a one-line cause and the next step. It stays until the next action. Errors never use modal alerts, except overwrite and discard confirmations. | Status line |
| Disabled | Controls render with egui's disabled styling and keep their position. Hovering shows a tooltip naming the precondition, such as "Add a Deepgram API key in Settings." | Toolbar |

### Dialogs

- The **settings sheet** is a modal egui window. It holds the output folder picker, and one masked API-key field per remote provider with Save and Remove buttons and a line naming the key's source. The toolbar selection is remembered as the default, so the sheet has no default-provider control. Focus moves to its first field on open and returns to the ⚙ button on close.
- **Confirmation dialogs** (overwrite, discard unsaved) name the consequence in the button label: "Replace files", "Discard transcript". Cancel is the default and responds to Esc.
- File and folder pickers are native dialogs (`rfd`).

---

## Adaptation and accessibility

- **Target.** Lalela commits to full keyboard operation, VoiceOver support through egui's AccessKit integration, and WCAG 2.2 Level AA contrast (1.4.3 Contrast (Minimum), 1.4.11 Non-text Contrast) as the colour bar in both appearances. WCAG is a web standard. Lalela uses its contrast criteria as a measurable threshold, not as a conformance claim for a native app.
- **Screen reader.** Every control has an accessible name. Each transcript segment is exposed as one item whose label reads "<timestamp>, <text>". Status-line changes are exposed as a live region where egui/AccessKit supports it.
- **Not colour alone.** The current segment combines the background tint with a leading bar. Errors combine colour with an "Error:" prefix.
- **Window size.** The minimum window is `WINDOW_WIDTH_PT_MIN` × `WINDOW_HEIGHT_PT_MIN` (520 × 360 pt). Below `TOOLBAR_WRAP_WIDTH_PT` (720 pt) wide, the toolbar wraps the pickers onto a second row; it never truncates button labels.
- **Text size.** ⌘= and ⌘− change the transcript font size between `TRANSCRIPT_FONT_PT_MIN` (12 pt) and `TRANSCRIPT_FONT_PT_MAX` (24 pt), and ⌘0 resets it. Only the transcript font changes; controls keep their size.
- **Content extremes.** Long file names truncate in the middle with the full path in a tooltip. Transcripts at `SEGMENTS_COUNT_MAX` render through a virtualised list (`ScrollArea::show_rows`). Right-to-left transcript text renders as egui shapes it, and full bidirectional support is not claimed.
- **Reduced motion.** When macOS "Reduce motion" is on, auto-scroll jumps instead of animating.

---

## Motion

Lalela's interface is static except for three functional motions: the progress indicator, the playhead, and transcript auto-scroll. Auto-scroll uses egui's default scroll animation and becomes an instant jump under Reduce motion. There are no decorative transitions.

---

## Content and UX voice

- Sentence case for every label and title.
- Buttons are verbs: Open, Transcribe, Cancel, Save, Replace files.
- Providers use their product names: "Apple (on-device)", "Mistral Voxtral", "Deepgram".
- Errors state what happened, then what to do: "Deepgram rejected the API key. Update it in Settings."
- Times use `m:ss`, or `h:mm:ss` from one hour, in the transcript and the player.
- No exclamation points and no emoji.

---

## Design definition of done

| Requirement | Scope and expected result | Evidence | Existing enforcement |
|---|---|---|---|
| Keyboard reachability | Every action in the shortcut table works by keyboard, and Tab reaches every control in the documented order | Manual keyboard walk-through on macOS 27 | Manual review |
| Visible focus | Every focusable control shows the accent focus stroke in light and dark appearance | Rendered check in both appearances | Manual review |
| VoiceOver | Open → transcribe → play → save completes with VoiceOver on; controls and segments are announced by name | Manual VoiceOver walk-through; record macOS and egui versions | Manual review |
| Contrast | Text and the current-segment indicator meet WCAG 2.2 1.4.3 / 1.4.11 against their backgrounds with the default blue accent and the yellow accent, in light and dark | Colour measurement from renders | Manual review |
| State coverage | Each row in the States table renders as described | Renders of each state | Manual review |
| Long content | A 3-hour transcript at `SEGMENTS_COUNT_MAX` scrolls and highlights without dropped frames; a 200-character file name truncates in the middle | Rendered check with fixture | Manual review |
| Reduced motion | With Reduce motion on, auto-scroll jumps | Interaction check | Manual review |

Missing evidence stays `not tested`; it is never reported as passing.

---

## Assumptions and open questions

**Assumptions**

- egui 0.37's AccessKit integration exposes buttons, text, and scroll areas to VoiceOver on macOS.
- The macOS system font file is readable from the system font directory at runtime and loads in egui.
- The user has one display scale at a time. egui handles Retina scaling.

**Decisions**

- *Layout.* **Toolbar, transcript, player bar in one window.** The owner chose it. The transcript gets the most space, and the player sits where the eye drops after reading.
- *Visual identity.* **macOS-leaning with no brand colour.** The owner chose it. A utility app that matches the system appearance looks less out of place than a custom-themed egui window.
- *Accent source.* **System accent colour read through the shim.** egui cannot read `NSColor`. The shim already exists, so one more export costs less than a fixed colour that ignores user preference.
- *Errors in the status line.* **No modal error alerts.** Errors are frequent and recoverable (bad key, network). A modal blocks playback for no gain.
- *Accessibility commitment.* **Full keyboard plus VoiceOver, WCAG AA contrast.** The owner chose it. Keyboard is fully in Lalela's control; VoiceOver depends on AccessKit, so the gaps are tracked below.

**Open questions**

- *AccessKit coverage.* Does egui's AccessKit integration announce custom-painted widgets (the scrubber, the segment rows) and live-region status updates on macOS 27? This needs a VoiceOver spike before the UI is built.
- *Accent contrast.* Some macOS accent colours (yellow, graphite) may fail 1.4.11 against the light background. Should Lalela darken the accent for strokes when contrast falls below 3:1?
- *System font loading.* If the system font file is a variable font that egui cannot load, is egui's bundled font acceptable, or should Lalela bundle a fallback?
- *Automated UI checks.* No design checks are automated. Should `egui_kittest` snapshot tests cover the States table?
