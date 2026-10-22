//! The settings sheet (06-app-shell.md §Window regions).

use egui::{RichText, TextEdit};

use crate::ui::UiIntent;
use crate::view_model::ViewModel;

/// Render the settings sheet.
pub fn settings(ctx: &egui::Context, vm: &ViewModel, intents: &mut Vec<UiIntent>) {
    if !vm.settings_open {
        return;
    }
    let mut open = true;
    egui::Window::new("Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Output folder:");
                if let Some(folder) = &vm.settings.output_folder {
                    ui.label(RichText::new(folder.display().to_string()).weak());
                } else {
                    ui.label(RichText::new("None").weak());
                }
                if ui.button("Choose…").clicked() {
                    intents.push(UiIntent::ChooseOutputFolder);
                }
            });
            ui.separator();
            for key in &vm.settings.keys {
                ui.horizontal(|ui| {
                    ui.label(key.provider.display_name());
                    ui.label(RichText::new(&key.source).weak());

                    if ui.button("Remove").clicked() {
                        intents.push(UiIntent::RemoveApiKey(key.provider));
                    }
                });
                let input_id = egui::Id::unique(("key_input", key.provider.as_str()));
                let key_text =
                    ctx.data(|data| data.get_temp::<String>(input_id).unwrap_or_default());
                let mut key_text = key_text;
                let response = ui.add(
                    TextEdit::singleline(&mut key_text)
                        .password(true)
                        .hint_text("API key"),
                );
                if response.changed() {
                    ctx.data_mut(|data| data.insert_temp(input_id, key_text.clone()));
                }
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    if !key_text.is_empty() {
                        intents.push(UiIntent::SetApiKey(key.provider, key_text.clone()));
                        ctx.data_mut(|data| data.insert_temp(input_id, String::new()));
                    } else if ui.button("Remove").clicked() {
                        intents.push(UiIntent::RemoveApiKey(key.provider));
                    }
                }
            }
            ui.separator();
            if ui.button("Close").clicked() {
                intents.push(UiIntent::CloseSettings);
            }
        });
    if !open {
        intents.push(UiIntent::CloseSettings);
    }
}
