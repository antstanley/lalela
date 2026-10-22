//! The read-only view model (06-app-shell.md §View model).

use std::path::PathBuf;

use lalela_core::ports::{ProviderId, Readiness};
use lalela_core::transcript::Segment;
use lalela_core::units::Milliseconds;

use crate::progress::Phase;

/// What the primary toolbar button is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryAction {
    /// Open a file.
    Open,
    /// Transcribe.
    Transcribe,
    /// Cancel.
    Cancel,
    /// Save.
    Save,
    /// None.
    None,
}

/// The status line.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Idle.
    Idle,
    /// A job with progress.
    Progress {
        /// The phase.
        phase: Phase,
        /// Determinate fraction.
        fraction: Option<f32>,
        /// The provider name.
        provider: &'static str,
    },
    /// Cancelling.
    Cancelling,
    /// Unsaved transcript.
    Unsaved,
    /// Saved to a folder.
    Saved { folder: PathBuf },
    /// An error.
    Error { message: String },
}

/// One provider row in the toolbar.
#[derive(Clone, Debug)]
pub struct ProviderView {
    /// The provider id.
    pub id: ProviderId,
    /// Display name.
    pub display_name: &'static str,
    /// Readiness.
    pub readiness: Readiness,
    /// Models.
    pub models: Vec<ModelView>,
    /// The selected model id.
    pub selected_model: String,
}

/// One model row.
#[derive(Clone, Debug)]
pub struct ModelView {
    /// Model id.
    pub id: &'static str,
    /// Label.
    pub label: &'static str,
    /// Whether language hints are supported.
    pub supports_language_hint: bool,
}

/// The view model.
#[derive(Clone, Debug)]
pub struct ViewModel {
    /// The open file name.
    pub file_name: Option<String>,
    /// The audio duration.
    pub duration: Option<Milliseconds>,
    /// Providers.
    pub providers: Vec<ProviderView>,
    /// The selected provider.
    pub provider: ProviderId,
    /// The language preference label.
    pub language: String,
    /// Whether the language picker is enabled.
    pub language_enabled: bool,
    /// The primary action.
    pub primary_action: PrimaryAction,
    /// Segments.
    pub segments: Option<Vec<Segment>>,
    /// The highlighted segment index.
    pub current_segment: Option<u32>,
    /// The playhead.
    pub playhead: Milliseconds,
    /// Whether playing.
    pub is_playing: bool,
    /// The status line.
    pub status: Status,
    /// The accent colour.
    pub accent: [u8; 4],
    /// The transcript font size.
    pub transcript_font_pt: f32,
    /// Whether a settings sheet is open.
    pub settings_open: bool,
    /// Whether a confirmation sheet is open.
    pub confirm_open: bool,
    /// The confirmation prompt text.
    pub confirm_prompt: String,
    /// The confirm button label.
    pub confirm_accept: String,
    /// The settings sheet model.
    pub settings: SettingsView,
}

/// The settings sheet view.
#[derive(Clone, Debug)]
pub struct SettingsView {
    /// The output folder.
    pub output_folder: Option<PathBuf>,
    /// Key source labels per remote provider.
    pub keys: Vec<KeyView>,
}

/// One key row.
#[derive(Clone, Debug)]
pub struct KeyView {
    /// The provider.
    pub provider: ProviderId,
    /// The key source description.
    pub source: String,
}
