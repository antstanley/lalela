//! The transcript pane (04-playback.md §Highlight and auto-scroll).

use egui::{Color32, RichText, ScrollArea};

use crate::ui::UiIntent;
use crate::view_model::ViewModel;

/// Render the transcript; pushes intents.
pub fn transcript(ui: &mut egui::Ui, vm: &ViewModel, intents: &mut Vec<UiIntent>) {
    let Some(segments) = &vm.segments else {
        ui.centered_and_justified(|ui| {
            let message = if vm.file_name.is_some() {
                "Ready to transcribe."
            } else {
                "Open or drop an audio file."
            };
            ui.label(RichText::new(message).size(16.0).weak());
        });
        return;
    };

    let accent = Color32::from_rgba_unmultiplied(vm.accent[0], vm.accent[1], vm.accent[2], 40);
    let bar = Color32::from_rgba_unmultiplied(vm.accent[0], vm.accent[1], vm.accent[2], 255);

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show_rows(ui, 28.0, segments.len(), |ui, range| {
            for index in range {
                let Some(segment) = segments.get(index) else {
                    continue;
                };
                let highlighted = vm.current_segment == Some(index as u32);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 28.0),
                    egui::Sense::click(),
                );
                if ui
                    .interact(
                        rect,
                        egui::Id::unique(("segment", index)),
                        egui::Sense::click(),
                    )
                    .clicked()
                {
                    intents.push(UiIntent::SegmentActivate(index as u32));
                }
                if highlighted {
                    ui.painter().rect_filled(rect, 2.0, accent);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_max(
                            rect.min,
                            egui::pos2(rect.min.x + 3.0, rect.max.y),
                        ),
                        0.0,
                        bar,
                    );
                }
                let painter = ui.painter();
                painter.text(
                    egui::pos2(rect.min.x + 8.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    format_time(segment.start_ms.as_u64()),
                    egui::FontId::monospace(12.0),
                    Color32::GRAY,
                );
                painter.text(
                    egui::pos2(rect.min.x + 64.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    &segment.text,
                    egui::FontId::proportional(vm.transcript_font_pt),
                    ui.visuals().text_color(),
                );
            }
        });
}

/// `m:ss` or `h:mm:ss`.
#[must_use]
pub fn format_time(ms: u64) -> String {
    let total_seconds = ms / 1000;
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}
