//! The status line (design-guidelines.md §States).

use egui::RichText;

use crate::view_model::{Status, ViewModel};

/// Render the status line.
pub fn status(ui: &mut egui::Ui, vm: &ViewModel, intents: &mut Vec<crate::ui::UiIntent>) {
    match &vm.status {
        Status::Idle => {
            ui.label("Idle");
        }
        Status::Progress {
            phase,
            fraction,
            provider,
        } => {
            ui.horizontal(|ui| {
                let label = match phase {
                    crate::progress::Phase::PreparingModel => "Preparing model…",
                    crate::progress::Phase::Uploading => "Uploading…",
                    crate::progress::Phase::Processing => "Processing…",
                    crate::progress::Phase::None => "",
                };
                ui.spinner();
                ui.label(format!("{provider} {label}"));
                if let Some(fraction) = fraction {
                    ui.add(
                        egui::ProgressBar::new(*fraction)
                            .desired_width(120.0)
                            .text(format!("{:.0}%", fraction * 100.0)),
                    );
                }
            });
        }
        Status::Cancelling => {
            ui.label("Stopping…");
        }
        Status::Unsaved => {
            ui.label(RichText::new("Unsaved").strong());
        }
        Status::Saved { folder } => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(format!("Saved to {}", folder.display())).strong());
                if ui.link("Show in Finder").clicked() {
                    intents.push(crate::ui::UiIntent::ShowInFinder);
                }
            });
        }
        Status::Error { message } => {
            ui.label(RichText::new(message).color(egui::Color32::from_rgb(0xb3, 0x26, 0x26)));
        }
    }
}
