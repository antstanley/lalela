//! Startup and composition root (06-app-shell.md §Startup).

use std::sync::Arc;

use lalela_core::ports::{Appearance, SecretStore, SettingsStore, TranscriptStore};
use lalela_core::registry::ProviderRegistry;
use lalela_store::{FsTranscriptStore, JsonSettingsStore};

use crate::app::LalelaApp;
use crate::controller::Controller;

mod app;
mod controller;
mod platform;
mod progress;
mod ui;
mod view_model;
mod worker;

const WINDOW_WIDTH_PT_MIN: f32 = 520.0;
const WINDOW_HEIGHT_PT_MIN: f32 = 360.0;

/// The minimal stderr logger (architecture-principles.md §Logging).
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, _metadata: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, record: &log::Record<'_>) {
        eprintln!("[{}] {}", record.level(), record.args());
    }

    fn flush(&self) {}
}

static LOGGER: StderrLogger = StderrLogger;

fn main() -> eframe::Result {
    log::set_logger(&LOGGER).unwrap_or_else(|error| panic!("set logger failed: {error}"));
    log::set_max_level(log::LevelFilter::Info);

    let os_version = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default();
    let major = os_version
        .split('.')
        .next()
        .and_then(|part| part.parse::<u32>().ok())
        .unwrap_or(0);
    assert!(major >= 27, "Lalela requires macOS 27 or later");

    let http = lalela_http::HttpClient::new();

    let registry = Arc::new(ProviderRegistry::new(vec![
        Box::new(lalela_macos::AppleSpeechTranscriber::new()),
        Box::new(lalela_voxtral::VoxtralTranscriber::new(http.clone())),
        Box::new(lalela_deepgram::DeepgramTranscriber::new(http)),
    ]));

    let store: Arc<dyn TranscriptStore> = Arc::new(FsTranscriptStore::new());
    let settings_path = std::path::PathBuf::from(
        std::env::var("HOME").unwrap_or_else(|_| panic!("HOME is not set")),
    )
    .join("Library/Application Support/Lalela/settings.json");
    let settings_store: Arc<dyn SettingsStore> = Arc::new(JsonSettingsStore::new(settings_path));
    let secret: Arc<dyn SecretStore> = Arc::new(lalela_macos::KeychainSecretStore::new());
    let appearance: Arc<dyn Appearance> = Arc::new(lalela_macos::MacAppearance::new());
    let clock: Arc<dyn lalela_core::ports::Clock> = Arc::new(platform::SystemClock);
    let ids = Box::new(platform::UuidV7IdSource);
    let player = Box::new(lalela_macos::AvAudioPlayer::new());

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WINDOW_WIDTH_PT_MIN, WINDOW_HEIGHT_PT_MIN])
            .with_min_inner_size([WINDOW_WIDTH_PT_MIN, WINDOW_HEIGHT_PT_MIN]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "Lalela",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.options_mut(|options| {
                options.zoom_with_keyboard = false;
            });
            let controller = Controller::new(
                registry,
                store,
                player,
                secret,
                settings_store,
                clock,
                ids,
                appearance,
                cc.egui_ctx.clone(),
            );
            Ok(Box::new(LalelaApp::new(controller)))
        }),
    )
}
