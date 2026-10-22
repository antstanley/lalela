# 04 — Playback and Segment Highlighting

**Status:** Draft · **Date:** 2026-10-21 · **Owner:** ant@senzo.io

This page defines the `AudioPlayer` port, how the UI polls the playhead, and how the current transcript segment is found and highlighted. The AVFoundation implementation is in [03-macos-shim.md](03-macos-shim.md). The visual treatment of the highlight is in [design-guidelines.md](design-guidelines.md).

---

## Responsibilities

1. Load the session's audio file and report its duration, which also completes `AudioSource`.
2. Play, pause, and seek by keyboard, by scrubber, and by clicking a segment.
3. Report the playhead position each frame while playing, and find the segment under it.
4. Keep the current segment visible in the transcript pane without fighting the user's own scrolling.

Playback does **not** own: transcription, session state transitions, or decoding formats. AVFoundation does the decoding.

---

## The `AudioPlayer` port

Defined in `lalela-core/src/ports/player.rs`.

```rust
pub trait AudioPlayer {
    fn load(&mut self, path: &Path) -> Result<Milliseconds, PlayerError>;   // returns duration
    fn play(&mut self) -> Result<(), PlayerError>;
    fn pause(&mut self) -> Result<(), PlayerError>;
    fn seek(&mut self, position: Milliseconds) -> Result<(), PlayerError>;
    fn state(&self) -> Result<PlayheadState, PlayerError>;
    fn unload(&mut self);
}

pub struct PlayheadState { pub position: Milliseconds, pub is_playing: bool }

pub enum PlayerError { NotLoaded, FileNotFound, UnsupportedAudio, Platform { status: i32 } }
```

The port has no `Send` bound. `AvAudioPlayer` is `!Send` and lives in `SessionController` on the main thread.

`FakePlayer` in `lalela-core/src/testing.rs` advances a position from an injected clock. Controller tests use it.

---

## Opening audio

```
UiIntent::Open(path)
  → AudioFormat::from_path(path)?                 reject unknown extensions
  → metadata(path).len() ≤ AUDIO_BYTES_MAX?       reject oversize
  → player.load(path) → duration                  AVFoundation probes the file
  → AudioSource::try_new(path, format, size, duration)?   duration > 0 and ≤ AUDIO_DURATION_MS_MAX
  → SessionState::apply(Opened(source))
```

If `load` fails, the session stays where it was, and any previously loaded audio is reloaded. A file that the player cannot decode is never offered to a provider.

---

## Playhead polling

egui redraws on demand, so the playhead is polled, not pushed.

- Each frame, the controller calls `player.state()` once and caches `PlayheadState` for the views.
- While `is_playing` is true, the controller calls `ctx.request_repaint_after(PLAYHEAD_REPAINT_INTERVAL_MS)`. When paused, it schedules nothing, and the app is idle.
- When the position reaches the duration, AVFoundation stops. The next poll sees `is_playing == false`, and the play button shows ▶ again. Pressing play from the end seeks to 0 first.

---

## Finding the current segment

`segment_at_position(segments: &[Segment], position: Milliseconds) -> Option<u32>` in `lalela-core/src/playback.rs`:

1. Assert `segments` is non-empty and its `start_ms` values are non-decreasing. The second check runs only in debug builds because it is O(n); `Transcript` construction already guarantees it.
2. Binary-search for the last segment with `start_ms ≤ position`.
3. Return its index when `position < end_ms`. Otherwise return `None`, because the playhead is in a gap.
4. Assert the returned index is `< segments.len()`.

`segment_next(index)` and `segment_previous(index, position)` serve ⌥→ and ⌥←. "Previous" goes to the start of the current segment when the playhead is more than `SEGMENT_RESTART_THRESHOLD_MS` into it, and otherwise to the segment before. This matches track-skip behaviour in media players.

Property tests check, for random sorted segment lists and positions, that the result agrees with a linear scan.

---

## Highlight and auto-scroll

| Behaviour | Rule |
|---|---|
| Highlight | The segment at `segment_at_position` is drawn with the current-segment treatment. In a gap, nothing is highlighted. |
| Auto-scroll | When the highlighted index changes and the row is outside the visible rect, the view scrolls it into view with `Align::Center`. |
| User scroll wins | A user scroll (wheel, trackpad, scrollbar drag) records `last_user_scroll`. Auto-scroll is suppressed for `AUTOSCROLL_SUPPRESS_MS` afterwards. |
| Reduced motion | Under the macOS Reduce Motion setting, scrolling is instant. |
| Click to seek | Clicking a segment, or pressing Return on a focused one, calls `player.seek(segment.start_ms)` and keeps the play/pause state. |
| Large transcripts | The transcript pane uses `ScrollArea::show_rows` with a fixed row-height estimate, so a transcript at `SEGMENTS_COUNT_MAX` lays out only the visible rows. Long segments wrap; the estimate is recomputed when the pane width changes. |

---

## Player bar controls

| Control | Action | Port call |
|---|---|---|
| Play / pause button, Space | Toggle | `play` / `pause` |
| Scrubber drag | Seek on release; the handle follows the pointer while dragging | `seek` |
| Scrubber click | Seek to the clicked position | `seek` |
| ← / → | Seek −/+ `SEEK_STEP_MS`, clamped to `[0, duration]` | `seek` |
| Time labels | Position and duration as `m:ss` or `h:mm:ss` | — |

The scrubber is a custom-painted egui widget with an AccessKit node: role slider, value in seconds, and an accessible name of "Playback position".

---

## Assumptions and open questions

**Assumptions**

- A single `player.state()` FFI call per frame is negligible next to egui's frame cost.
- AVAudioPlayer stops at the end of the file and reports `isPlaying == false` without an extra callback.

**Decisions**

- *Polling over callbacks.* **Poll once per frame while playing.** egui is immediate-mode, so a callback would only set a flag that the next frame reads anyway. Polling has no cross-thread state.
- *Repaint cadence.* **`PLAYHEAD_REPAINT_INTERVAL_MS` = 33 while playing, none while paused.** About 30 Hz is smooth enough for a scrubber and a text highlight, and the app idles when paused.
- *Gap behaviour.* **No highlight in gaps.** Keeping the previous segment highlighted during long silences would imply speech that is not there.
- *Seek on segment click.* **Keep the play/pause state.** Clicking while paused is how users check where a segment starts without starting playback.

**Open questions**

- *Playback rate.* Should the player offer 0.75× / 1.25× / 1.5× speeds (`AVAudioPlayer.rate` with `enableRate`)? Transcript review often benefits from them, but this adds a control to a minimal UI.
