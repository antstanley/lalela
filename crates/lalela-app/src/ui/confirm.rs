//! The confirmation sheet (06-app-shell.md §Window regions).

use crate::ui::{ConfirmChoice, UiIntent};
use crate::view_model::ViewModel;

/// Render the confirmation sheet.
pub fn confirm(ctx: &egui::Context, vm: &ViewModel, intents: &mut Vec<UiIntent>) {
    if !vm.confirm_open {
        return;
    }
    egui::Window::new("Confirm")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.label(&vm.confirm_prompt);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    intents.push(UiIntent::Confirm(ConfirmChoice::Cancel));
                }
                if ui.button(&vm.confirm_accept).clicked() {
                    intents.push(UiIntent::Confirm(ConfirmChoice::Proceed));
                }
            });
        });
}
