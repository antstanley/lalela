//! `JsonSettingsStore`: settings load/save with defaults and invalid-file
//! quarantine (05-persistence.md §Settings).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use lalela_core::ports::{ProviderId, Settings, SettingsError, SettingsStore};
use lalela_core::units::{LanguagePreference, LanguageTag};
use serde::{Deserialize, Serialize};

use crate::limits::SETTINGS_BYTES_MAX;

/// The settings document shape. `deny_unknown_fields` rejects unknown keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SettingsDocument {
    /// Schema version; must be 1.
    pub schema_version: u32,
    /// The output folder (absolute), or null.
    pub output_folder: Option<String>,
    /// The provider string form.
    pub provider: String,
    /// One model id per provider.
    pub models: BTreeMap<String, String>,
    /// `"auto"` or a language tag.
    pub language: String,
}

const PROVIDER_KEYS: [&str; 3] = ["apple-speech", "mistral-voxtral", "deepgram"];

/// A JSON settings store at a fixed path.
#[derive(Debug, Clone)]
pub struct JsonSettingsStore {
    path: PathBuf,
}

impl JsonSettingsStore {
    /// Build a store for `path`.
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// The settings file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Parse a document into core `Settings`, applying the schema rules.
    fn parse(document: SettingsDocument) -> Result<Settings, SettingsError> {
        if document.schema_version != 1 {
            return Err(SettingsError::Invalid {
                detail: format!("unsupported schema_version: {}", document.schema_version),
            });
        }
        let provider = ProviderId::from_str_form(&document.provider).ok_or_else(|| {
            SettingsError::Invalid {
                detail: format!("unknown provider: {}", document.provider),
            }
        })?;
        let language = if document.language == "auto" {
            LanguagePreference::Auto
        } else {
            LanguagePreference::Tag(LanguageTag::try_new(document.language.clone()).ok_or_else(
                || SettingsError::Invalid {
                    detail: format!("invalid language tag: {}", document.language),
                },
            )?)
        };
        let mut models = BTreeMap::new();
        for key in PROVIDER_KEYS {
            let model = document
                .models
                .get(key)
                .ok_or_else(|| SettingsError::Invalid {
                    detail: format!("missing model for provider {key}"),
                })?;
            let provider_id = ProviderId::from_str_form(key).unwrap_or_else(|| {
                panic!("provider keys are known constants");
            });
            models.insert(provider_id, model.clone());
        }
        let output_folder = match document.output_folder {
            None => None,
            Some(path) if path.starts_with('/') => Some(PathBuf::from(path)),
            Some(path) => {
                return Err(SettingsError::Invalid {
                    detail: format!("output_folder is not absolute: {path}"),
                });
            }
        };
        Ok(Settings {
            output_folder,
            provider,
            models,
            language,
        })
    }

    /// Encode settings as a document.
    fn encode(settings: &Settings) -> SettingsDocument {
        let mut models = BTreeMap::new();
        for key in PROVIDER_KEYS {
            let provider_id = ProviderId::from_str_form(key).unwrap_or_else(|| {
                panic!("provider keys are known constants");
            });
            if let Some(model) = settings.models.get(&provider_id) {
                models.insert(key.to_owned(), model.clone());
            }
        }
        SettingsDocument {
            schema_version: 1,
            output_folder: settings
                .output_folder
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
            provider: settings.provider.as_str().to_owned(),
            models,
            language: match &settings.language {
                LanguagePreference::Auto => "auto".to_owned(),
                LanguagePreference::Tag(tag) => tag.as_str().to_owned(),
            },
        }
    }
}

impl SettingsStore for JsonSettingsStore {
    fn load(&self) -> Result<Settings, SettingsError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(SettingsError::NotFound);
            }
            Err(error) => {
                return Err(SettingsError::Io {
                    detail: format!("read settings failed: {error}"),
                });
            }
        };
        if bytes.len() as u64 > SETTINGS_BYTES_MAX {
            return Err(SettingsError::Invalid {
                detail: "settings file exceeds SETTINGS_BYTES_MAX".to_owned(),
            });
        }
        let document: SettingsDocument =
            serde_json::from_slice(&bytes).map_err(|error| SettingsError::Invalid {
                detail: format!("settings decode failed: {error}"),
            })?;
        Self::parse(document)
    }

    fn save(&self, settings: &Settings) -> Result<(), SettingsError> {
        let document = Self::encode(settings);
        let bytes = serde_json::to_vec_pretty(&document).map_err(|error| SettingsError::Io {
            detail: format!("settings encode failed: {error}"),
        })?;
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| SettingsError::Io {
                detail: format!("create settings dir failed: {error}"),
            })?;
        }
        crate::store::FsTranscriptStore::write_file_atomic(&self.path, &bytes).map_err(|error| {
            SettingsError::Io {
                detail: format!("save settings failed: {error:?}"),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_defaults() {
        let document = SettingsDocument {
            schema_version: 1,
            output_folder: Some("/Users/me/Transcripts".to_owned()),
            provider: "deepgram".to_owned(),
            models: BTreeMap::from([
                ("apple-speech".to_owned(), "speech-transcriber".to_owned()),
                ("mistral-voxtral".to_owned(), "voxtral-mini-2602".to_owned()),
                ("deepgram".to_owned(), "nova-3".to_owned()),
            ]),
            language: "auto".to_owned(),
        };
        let settings = JsonSettingsStore::parse(document).unwrap();
        assert_eq!(settings.provider, ProviderId::Deepgram);
        assert_eq!(settings.language, LanguagePreference::Auto);
        assert_eq!(
            settings.output_folder.unwrap(),
            PathBuf::from("/Users/me/Transcripts")
        );
    }

    #[test]
    fn parse_rejects_unknown_provider() {
        let document = SettingsDocument {
            schema_version: 1,
            output_folder: None,
            provider: "wat".to_owned(),
            models: BTreeMap::from([
                ("apple-speech".to_owned(), "speech-transcriber".to_owned()),
                ("mistral-voxtral".to_owned(), "voxtral-mini-2602".to_owned()),
                ("deepgram".to_owned(), "nova-3".to_owned()),
            ]),
            language: "auto".to_owned(),
        };
        assert!(JsonSettingsStore::parse(document).is_err());
    }

    #[test]
    fn parse_rejects_relative_output_folder() {
        let document = SettingsDocument {
            schema_version: 1,
            output_folder: Some("Transcripts".to_owned()),
            provider: "deepgram".to_owned(),
            models: BTreeMap::from([
                ("apple-speech".to_owned(), "speech-transcriber".to_owned()),
                ("mistral-voxtral".to_owned(), "voxtral-mini-2602".to_owned()),
                ("deepgram".to_owned(), "nova-3".to_owned()),
            ]),
            language: "auto".to_owned(),
        };
        assert!(JsonSettingsStore::parse(document).is_err());
    }

    #[test]
    fn round_trip_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let store = JsonSettingsStore::new(path.clone());
        let settings = Settings::defaults();
        store.save(&settings).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded, settings);
    }

    #[test]
    fn missing_file_is_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonSettingsStore::new(dir.path().join("missing.json"));
        assert_eq!(store.load(), Err(SettingsError::NotFound));
    }
}
