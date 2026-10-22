//! The `SettingsStore`, `SecretStore`, `Clock`, `IdSource`, and `Appearance`
//! ports (05-persistence.md, 06-app-shell.md).

use std::path::PathBuf;

use crate::ports::transcriber::ProviderId;
use crate::units::{ApiKey, LanguagePreference};

/// User preferences that persist between launches.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    /// The output folder; `None` until the user chooses one.
    pub output_folder: Option<PathBuf>,
    /// The provider last selected in the toolbar.
    pub provider: ProviderId,
    /// One model id per provider, keyed by provider id.
    pub models: std::collections::BTreeMap<ProviderId, String>,
    /// The language preference.
    pub language: LanguagePreference,
}

impl Settings {
    /// Default settings: Apple Speech, its default model, automatic language.
    #[must_use]
    pub fn defaults() -> Self {
        let mut models = std::collections::BTreeMap::new();
        models.insert(ProviderId::AppleSpeech, "speech-transcriber".to_owned());
        models.insert(ProviderId::MistralVoxtral, "voxtral-mini-2602".to_owned());
        models.insert(ProviderId::Deepgram, "nova-3".to_owned());
        Self {
            output_folder: None,
            provider: ProviderId::AppleSpeech,
            models,
            language: LanguagePreference::Auto,
        }
    }

    /// The stored model id for a provider.
    #[must_use]
    pub fn model(&self, provider: ProviderId) -> Option<&str> {
        self.models.get(&provider).map(String::as_str)
    }

    /// Store a model id for a provider.
    pub fn set_model(&mut self, provider: ProviderId, model: &str) {
        self.models.insert(provider, model.to_owned());
    }
}

/// A settings load/save failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsError {
    /// No settings file exists; the caller uses defaults.
    NotFound,
    /// The settings file is invalid.
    Invalid { detail: String },
    /// An I/O failure.
    Io { detail: String },
}

/// The port that loads and saves settings.
pub trait SettingsStore {
    /// Load settings. `NotFound` means defaults.
    fn load(&self) -> Result<Settings, SettingsError>;

    /// Save settings atomically.
    fn save(&self, settings: &Settings) -> Result<(), SettingsError>;
}

/// Where a stored API key came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySource {
    /// The environment variable.
    Environment,
    /// The Keychain.
    Keychain,
}

/// A stored API key and its source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredKey {
    /// The key.
    pub key: ApiKey,
    /// Where it came from.
    pub source: KeySource,
}

/// A Keychain failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SecretError {
    /// A stored value is not a valid key.
    Invalid,
    /// The user refused the Keychain prompt.
    Denied,
    /// A platform failure.
    Platform { status: i32 },
}

/// The port that stores API keys per provider.
pub trait SecretStore: Send + Sync {
    /// Get the key for a provider: environment first, then the Keychain.
    fn get(&self, provider: ProviderId) -> Result<Option<StoredKey>, SecretError>;

    /// Store a key for a provider, overwriting any existing value.
    fn set(&self, provider: ProviderId, key: &ApiKey) -> Result<(), SecretError>;

    /// Delete the stored key for a provider.
    fn delete(&self, provider: ProviderId) -> Result<(), SecretError>;
}

/// The wall clock, as Unix milliseconds.
pub trait Clock: Send + Sync {
    /// The current time in milliseconds since the Unix epoch, UTC.
    fn now_unix_ms(&self) -> u64;
}

/// A source of new entity IDs.
pub trait IdSource {
    /// The next transcript ID.
    fn next_transcript_id(&self) -> crate::id::TranscriptId;
}

/// The system accent colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppearanceError {
    /// A platform failure.
    Platform { status: i32 },
}

/// A source of the system accent colour.
pub trait Appearance {
    /// The accent colour as RGBA bytes.
    fn accent_rgba(&self) -> Result<[u8; 4], AppearanceError>;
}
