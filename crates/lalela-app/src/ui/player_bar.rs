//! The player bar (04-playback.md §Player bar controls).

use egui::RichText;

use crate::ui::UiIntent;
use crate::ui::transcript::format_time;
use crate::view_model::ViewModel;

/// Render the player bar; pushes intents.
pub fn player_bar(ui: &mut egui::Ui, vm: &ViewModel, intents: &mut Vec<UiIntent>) {
    let has_audio = vm.file_name.is_some();
    ui.horizontal(|ui| {
        let play_label = if vm.is_playing { "❚❚" } else { "▶" };
        if ui
            .add_enabled(
                has_audio,
                egui::Button::new(RichText::new(play_label).size(18.0)),
            )
            .clicked()
        {
            intents.push(UiIntent::PlayPause);
        }

        let position = vm.playhead.as_u64();
        let duration = vm.duration.map(|d| d.as_u64()).unwrap_or(0);
        ui.label(RichText::new(format_time(position)).monospace());

        let mut value = position;
        let slider = egui::Slider::new(&mut value, 0..=duration.max(1))
            .show_value(false)
            .custom_formatter(|_value, _| String::new());
        let response = ui.add_enabled(has_audio, slider);
        if response.changed() {
            let value = value.min(duration);
            if let Some(ms) = lalela_core::units::Milliseconds::try_new(value) {
                intents.push(UiIntent::Seek(ms));
            }
        }

        ui.label(RichText::new(format_time(duration)).monospace());
    });
}
