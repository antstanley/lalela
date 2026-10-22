//! Keyboard shortcuts (design-guidelines.md §Keyboard).

use crate::ui::{ConfirmChoice, UiIntent};

/// Translate the frame's input into intents. Returns the intents and whether
/// a text field has focus (which suppresses Space and arrow keys).
pub fn shortcuts(ctx: &egui::Context) -> Vec<UiIntent> {
    let mut intents = Vec::new();
    let text_focused = ctx.memory(|memory| {
        memory.focused().is_some_and(|id| {
            // Heuristic: the focused widget is in the settings sheet text
            // fields when the sheet is open and the id is a TextEdit id.
            id.short_debug_format().contains("text_edit")
        })
    });

    ctx.input(|input| {
        let cmd = input.modifiers.mac_cmd || input.modifiers.command;
        let alt = input.modifiers.alt;
        let key = |k: egui::Key| input.key_pressed(k);
        let with_cmd = |k: egui::Key| cmd && key(k);
        let with_alt = |k: egui::Key| alt && key(k);

        if with_cmd(egui::Key::O) {
            intents.push(UiIntent::OpenDialog);
        }
        if with_cmd(egui::Key::Enter) {
            intents.push(UiIntent::Transcribe);
        }
        if with_cmd(egui::Key::Period) {
            intents.push(UiIntent::Cancel);
        }
        if with_cmd(egui::Key::S) {
            intents.push(UiIntent::Save);
        }
        if with_cmd(egui::Key::Comma) {
            intents.push(UiIntent::OpenSettings);
        }
        if with_cmd(egui::Key::Equals) {
            intents.push(UiIntent::TranscriptTextLarger);
        }
        if with_cmd(egui::Key::Minus) {
            intents.push(UiIntent::TranscriptTextSmaller);
        }
        if with_cmd(egui::Key::Num0) {
            intents.push(UiIntent::TranscriptTextReset);
        }
        if with_alt(egui::Key::ArrowRight) {
            intents.push(UiIntent::SegmentNext);
        }
        if with_alt(egui::Key::ArrowLeft) {
            intents.push(UiIntent::SegmentPrevious);
        }
        if !text_focused {
            if key(egui::Key::Space) {
                intents.push(UiIntent::PlayPause);
            }
            if key(egui::Key::ArrowLeft) {
                intents.push(UiIntent::SeekBy(
                    -(lalela_core::limits::SEEK_STEP_MS as i64),
                ));
            }
            if key(egui::Key::ArrowRight) {
                intents.push(UiIntent::SeekBy(lalela_core::limits::SEEK_STEP_MS as i64));
            }
        }
        if key(egui::Key::Escape) {
            intents.push(UiIntent::Confirm(ConfirmChoice::Cancel));
            intents.push(UiIntent::CloseSettings);
        }
    });
    intents
}
