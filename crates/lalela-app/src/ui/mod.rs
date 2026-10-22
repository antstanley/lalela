//! Pure views: they read the view model and emit intents
//! (architecture-principles.md §What goes where).

pub mod confirm;
pub mod messages;
pub mod player_bar;
pub mod settings;
pub mod shortcuts;
pub mod status;
pub mod theme;
pub mod toolbar;
pub mod transcript;

use std::path::PathBuf;

use lalela_core::ports::ProviderId;
use lalela_core::units::{LanguagePreference, Milliseconds};

/// A user action, emitted by the views and applied by the controller.
#[derive(Clone, Debug)]
pub enum UiIntent {
    /// Open the file dialog.
    OpenDialog,
    /// Open a specific path.
    OpenPath(PathBuf),
    /// Select a provider.
    SelectProvider(ProviderId),
    /// Select a model.
    SelectModel(&'static str),
    /// Select a language.
    SelectLanguage(LanguagePreference),
    /// Start transcription.
    Transcribe,
    /// Cancel the running transcription.
    Cancel,
    /// Save.
    Save,
    /// Reveal the saved pair in Finder.
    ShowInFinder,
    /// Toggle play/pause.
    PlayPause,
    /// Seek to a position.
    Seek(Milliseconds),
    /// Seek by a relative amount (negative for back).
    SeekBy(i64),
    /// Go to the next segment.
    SegmentNext,
    /// Go to the previous segment.
    SegmentPrevious,
    /// Activate (seek to) a segment.
    SegmentActivate(u32),
    /// Increase transcript font size.
    TranscriptTextLarger,
    /// Decrease transcript font size.
    TranscriptTextSmaller,
    /// Reset transcript font size.
    TranscriptTextReset,
    /// Open the settings sheet.
    OpenSettings,
    /// Close the settings sheet.
    CloseSettings,
    /// Choose the output folder.
    ChooseOutputFolder,
    /// Set an API key.
    SetApiKey(ProviderId, String),
    /// Remove an API key.
    RemoveApiKey(ProviderId),
    /// Answer a confirmation.
    Confirm(ConfirmChoice),
}

/// A confirmation answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfirmChoice {
    /// Proceed.
    Proceed,
    /// Cancel.
    Cancel,
}
