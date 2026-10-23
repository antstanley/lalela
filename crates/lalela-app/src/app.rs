//! The eframe app (06-app-shell.md §Window regions).

use crate::controller::Controller;
use crate::ui::{self, UiIntent};
use crate::view_model::ViewModel;

/// The app.
pub struct LalelaApp {
    controller: Controller,
    view_model: ViewModel,
}

impl LalelaApp {
    /// Build the app.
    pub fn new(controller: Controller) -> Self {
        let view_model = controller.view_model();
        Self {
            controller,
            view_model,
        }
    }
}

impl Drop for LalelaApp {
    fn drop(&mut self) {
        self.controller.shutdown();
    }
}

impl eframe::App for LalelaApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        self.controller.poll();

        if ctx.input(|i| i.viewport().close_requested()) {
            self.controller.request_quit();
            if !self.controller.quit_pending() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }

        let mut intents: Vec<UiIntent> = ui::shortcuts::shortcuts(&ctx);

        // Drag and drop: open the first dropped file.
        let dropped = ctx.input(|input| input.raw.dropped_files.clone());
        if let Some(first) = dropped.first() {
            intents.push(UiIntent::OpenPath(first.path().to_path_buf()));
        }

        let vm = self.controller.view_model();
        self.view_model = vm.clone();

        // Polled playback and job progress need frames even without input
        // (04-playback.md §Playhead polling, 06-app-shell.md §Worker).
        if vm.is_playing {
            ctx.request_repaint_after(std::time::Duration::from_millis(
                lalela_core::limits::PLAYHEAD_REPAINT_INTERVAL_MS,
            ));
        }
        if matches!(vm.status, crate::view_model::Status::Progress { .. })
            || matches!(vm.status, crate::view_model::Status::Cancelling)
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(
                lalela_core::limits::PROGRESS_REPAINT_INTERVAL_MS,
            ));
        }

        ctx.style_mut_of(ctx.theme(), |style| {
            ui::theme::apply_accent(&mut style.visuals, vm.accent);
        });

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui::toolbar::toolbar(ui, &vm, &mut intents);
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui::status::status(ui, &vm, &mut intents);
        });
        egui::Panel::bottom("player").show(ui, |ui| {
            ui::player_bar::player_bar(ui, &vm, &mut intents);
        });
        egui::CentralPanel::default().show(ui, |ui| {
            ui::transcript::transcript(ui, &vm, &mut intents);
        });

        ui::settings::settings(&ctx, &vm, &mut intents);
        ui::confirm::confirm(&ctx, &vm, &mut intents);

        debug_assert!(
            intents.len() <= lalela_core::limits::INTENTS_PER_FRAME_MAX,
            "too many intents in one frame"
        );
        for intent in intents {
            self.controller.apply(intent);
        }

        // A confirmed quit (through the discard confirmation sheet) issues the
        // close command — the original close request was consumed.
        if self.controller.consume_quit_confirmed() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if self.controller.quit_pending() {
            self.controller.request_quit();
            if !self.controller.quit_pending() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
}
