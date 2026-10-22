//! The `Transcriber` port: one interface for every transcription provider
//! (02-transcription-providers.md).

use crate::audio::AudioSource;
use crate::id::TranscriptId;
use crate::units::{ApiKey, Bytes, LanguagePreference};

/// A transcription provider identifier. This is a closed enum with stable
/// string forms; every `match` on it must name all variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ProviderId {
    /// Apple on-device Speech.
    AppleSpeech,
    /// Mistral Voxtral.
    MistralVoxtral,
    /// Deepgram.
    Deepgram,
}

impl ProviderId {
    /// The stable string form used in files and Keychain account names.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AppleSpeech => "apple-speech",
            Self::MistralVoxtral => "mistral-voxtral",
            Self::Deepgram => "deepgram",
        }
    }

    /// The display name shown in the UI.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::AppleSpeech => "Apple (on-device)",
            Self::MistralVoxtral => "Mistral Voxtral",
            Self::Deepgram => "Deepgram",
        }
    }

    /// Parse a stable string form. Unknown strings return `None`.
    #[must_use]
    pub fn from_str_form(value: &str) -> Option<Self> {
        match value {
            "apple-speech" => Some(Self::AppleSpeech),
            "mistral-voxtral" => Some(Self::MistralVoxtral),
            "deepgram" => Some(Self::Deepgram),
            _ => None,
        }
    }
}

/// One selectable model of a provider.
#[derive(Clone, Copy, Debug)]
pub struct ModelOption {
    /// The model id as sent to the provider.
    pub id: &'static str,
    /// The label shown in the UI.
    pub label: &'static str,
    /// Whether this model accepts an explicit language hint.
    pub supports_language_hint: bool,
}

/// Static, immutable facts about a provider.
#[derive(Debug)]
pub struct ProviderDescriptor {
    /// The provider id.
    pub id: ProviderId,
    /// Display name.
    pub display_name: &'static str,
    /// Whether the provider needs an API key.
    pub requires_api_key: bool,
    /// The models this provider offers (1..=`MODELS_PER_PROVIDER_MAX`).
    pub models: &'static [ModelOption],
    /// The model id selected by default.
    pub model_default: &'static str,
    /// The largest audio file this provider accepts, in bytes.
    pub audio_bytes_max: u64,
    /// The longest audio this provider accepts, in milliseconds.
    pub audio_duration_ms_max: u64,
}

/// A request to transcribe one audio file.
pub struct TranscriptionRequest<'a> {
    /// The transcript ID; also seeds the Voxtral multipart boundary.
    pub transcript_id: &'a TranscriptId,
    /// The audio to transcribe.
    pub audio: &'a AudioSource,
    /// The selected model.
    pub model: &'a ModelOption,
    /// The language preference.
    pub language: &'a LanguagePreference,
    /// The API key, when the provider requires one.
    pub api_key: Option<&'a ApiKey>,
}

/// Progress reported by a provider while a transcription runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Apple is installing on-device assets; permille of the download.
    PreparingModel { permille: u16 },
    /// Uploading the audio; bytes sent of total.
    Uploading { sent: Bytes, total: Bytes },
    /// Server-side processing; `None` means indeterminate.
    Processing { permille: Option<u16> },
}

/// A provider's readiness to transcribe, queried on the main thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Readiness {
    /// Ready to transcribe.
    Ready,
    /// A key is missing.
    NeedsApiKey,
    /// Unavailable for a static reason.
    Unavailable { reason: &'static str },
}

/// A transcription error. Every variant carries enough typed detail for the
/// UI to show one sentence; `detail` is truncated to
/// `ERROR_DETAIL_BYTES_MAX` and never includes request headers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TranscribeError {
    /// The observer cancelled the job.
    Cancelled,
    /// The audio file cannot be opened or read.
    FileUnreadable,
    /// The provider is unavailable on this machine.
    ProviderUnavailable { reason: String },
    /// HTTP 401 / 403.
    Unauthorized,
    /// HTTP 429 after retries.
    RateLimited,
    /// HTTP 413 or the file exceeds the provider's size limit.
    PayloadTooLarge,
    /// HTTP 415 or the shim's `UnsupportedAudio`.
    UnsupportedFormat,
    /// The requested language is not supported.
    UnsupportedLanguage,
    /// The selected model is not available.
    ModelUnavailable,
    /// A valid response with zero non-empty segments.
    NoSpeech,
    /// The request timed out.
    Timeout,
    /// A network failure after retries.
    Network { detail: String },
    /// Any other non-2xx status.
    Provider { status: u16, message: String },
    /// A response that failed to decode or violated an invariant.
    MalformedResponse { detail: String },
    /// A shim-internal failure.
    PlatformFailure { status: i32 },
}

/// The observer a provider reports progress and cancellation through. It is
/// `Sync` because the provider calls it from other threads (or, for the HTTP
/// providers, from the worker thread that owns the call).
pub trait TranscriptionObserver: Sync {
    /// Report progress.
    fn on_progress(&self, progress: Progress);
    /// Whether the job has been cancelled.
    fn is_cancelled(&self) -> bool;
}

/// The port every transcription provider implements.
pub trait Transcriber: Send + Sync {
    /// The provider's static descriptor.
    fn descriptor(&self) -> &'static ProviderDescriptor;

    /// Whether the provider can transcribe right now. Cheap and non-blocking.
    fn readiness(&self, api_key_present: bool) -> Readiness;

    /// Transcribe one audio file. Blocking; runs only on the worker thread.
    fn transcribe(
        &self,
        request: &TranscriptionRequest<'_>,
        observer: &dyn TranscriptionObserver,
    ) -> Result<crate::transcript::TranscriptDraft, TranscribeError>;
}
