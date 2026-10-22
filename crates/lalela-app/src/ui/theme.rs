//! The theme module (design-guidelines.md §Context and authority).

use egui::{Color32, Visuals};

/// Apply the accent colour to the visuals.
pub fn apply_accent(visuals: &mut Visuals, accent: [u8; 4]) {
    let accent = Color32::from_rgba_unmultiplied(accent[0], accent[1], accent[2], accent[3]);
    visuals.selection.bg_fill = accent;
    visuals.selection.stroke.color = accent;
    visuals.widgets.inactive.fg_stroke.color = visuals.text_color();
}
