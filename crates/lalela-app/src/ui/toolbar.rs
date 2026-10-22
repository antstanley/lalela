//! The toolbar (06-app-shell.md §Window regions).

use egui::{ComboBox, RichText};

use lalela_core::units::LanguagePreference;

use crate::ui::UiIntent;
use crate::view_model::{PrimaryAction, ViewModel};

/// Render the toolbar; pushes intents.
pub fn toolbar(ui: &mut egui::Ui, vm: &ViewModel, intents: &mut Vec<UiIntent>) {
    let job_running = vm.primary_action == PrimaryAction::Cancel
        || vm.primary_action == PrimaryAction::None
            && matches!(vm.status, crate::view_model::Status::Cancelling);

    ui.horizontal_wrapped(|ui| {
        let open_enabled = !job_running;
        if ui
            .add_enabled(open_enabled, egui::Button::new("Open"))
            .clicked()
        {
            intents.push(UiIntent::OpenDialog);
        }

        if let Some(file_name) = &vm.file_name {
            ui.label(RichText::new(file_name).weak());
        }

        let provider_enabled = !job_running;
        ui.add_enabled_ui(provider_enabled, |ui| {
            ComboBox::from_id_salt("provider")
                .selected_text(vm.provider.display_name())
                .show_ui(ui, |ui| {
                    for provider in &vm.providers {
                        let response =
                            ui.selectable_label(vm.provider == provider.id, provider.display_name);
                        let response = if !matches!(
                            provider.readiness,
                            lalela_core::ports::Readiness::Ready
                        ) {
                            let reason = match provider.readiness {
                                lalela_core::ports::Readiness::NeedsApiKey => {
                                    "Requires an API key in Settings"
                                }
                                lalela_core::ports::Readiness::Unavailable { reason } => reason,
                                lalela_core::ports::Readiness::Ready => "",
                            };
                            response.on_hover_text(reason)
                        } else {
                            response
                        };
                        if response.clicked() {
                            intents.push(UiIntent::SelectProvider(provider.id));
                        }
                    }
                });
        });

        if let Some(selected) = vm.providers.iter().find(|p| p.id == vm.provider) {
            let models_enabled = provider_enabled && !selected.models.is_empty();
            ui.add_enabled_ui(models_enabled, |ui| {
                ComboBox::from_id_salt("model")
                    .selected_text(&selected.selected_model)
                    .show_ui(ui, |ui| {
                        for model in &selected.models {
                            let response = ui
                                .selectable_label(selected.selected_model == model.id, model.label);
                            let response = if !model.supports_language_hint {
                                response.on_hover_text("No language hint for this model")
                            } else {
                                response
                            };
                            if response.clicked() {
                                intents.push(UiIntent::SelectModel(model.id));
                            }
                        }
                    });
            });
        }

        let language_enabled = provider_enabled && vm.language_enabled && !job_running;
        ui.add_enabled_ui(language_enabled, |ui| {
            ComboBox::from_id_salt("language")
                .selected_text(&vm.language)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(vm.language == "Auto", "Auto").clicked() {
                        intents.push(UiIntent::SelectLanguage(LanguagePreference::Auto));
                    }
                    for tag in ["en", "en-US", "fr", "de", "es", "pt-BR", "zh-Hant-TW", "af"] {
                        if ui.selectable_label(false, tag).clicked() {
                            let tag = lalela_core::units::LanguageTag::try_new(tag.to_owned());
                            if let Some(tag) = tag {
                                intents
                                    .push(UiIntent::SelectLanguage(LanguagePreference::Tag(tag)));
                            }
                        }
                    }
                });
        });

        match vm.primary_action {
            PrimaryAction::Open => {
                if ui.button("Open").clicked() {
                    intents.push(UiIntent::OpenDialog);
                }
            }
            PrimaryAction::Transcribe => {
                let enabled = !job_running;
                if ui
                    .add_enabled(enabled, egui::Button::new("Transcribe"))
                    .clicked()
                {
                    intents.push(UiIntent::Transcribe);
                }
            }
            PrimaryAction::Cancel => {
                if ui.button("Cancel").clicked() {
                    intents.push(UiIntent::Cancel);
                }
            }
            PrimaryAction::Save => {
                if ui.button("Save").clicked() {
                    intents.push(UiIntent::Save);
                }
            }
            PrimaryAction::None => {
                if matches!(vm.status, crate::view_model::Status::Cancelling) {
                    ui.add_enabled(false, egui::Button::new("Cancelling…"));
                }
            }
        }

        if ui.button("⚙").clicked() {
            intents.push(UiIntent::OpenSettings);
        }
    });
}
