//! The provider registry (02-transcription-providers.md §Provider registry).

use std::collections::BTreeMap;

use crate::limits::PROVIDERS_COUNT_MAX;
use crate::ports::{ProviderDescriptor, ProviderId, Transcriber};

/// Holds every registered provider. Built once at startup and shared between
/// the main thread and the worker as an `Arc`.
pub struct ProviderRegistry {
    providers: Vec<Box<dyn Transcriber>>,
}

impl ProviderRegistry {
    /// Build the registry. Asserts that every `ProviderId` variant is
    /// registered exactly once and that the count is within
    /// `PROVIDERS_COUNT_MAX`, so a new variant without an adapter fails at
    /// startup.
    pub fn new(providers: Vec<Box<dyn Transcriber>>) -> Self {
        assert!(
            providers.len() <= PROVIDERS_COUNT_MAX,
            "provider count exceeds PROVIDERS_COUNT_MAX"
        );
        let mut seen: BTreeMap<ProviderId, ()> = BTreeMap::new();
        for provider in &providers {
            let id = provider.descriptor().id;
            assert!(
                seen.insert(id, ()).is_none(),
                "provider {id:?} registered twice"
            );
        }
        for id in [
            ProviderId::AppleSpeech,
            ProviderId::MistralVoxtral,
            ProviderId::Deepgram,
        ] {
            assert!(seen.contains_key(&id), "provider {id:?} not registered");
        }
        Self { providers }
    }

    /// The provider with `id`.
    #[must_use]
    pub fn get(&self, id: ProviderId) -> Option<&dyn Transcriber> {
        self.providers
            .iter()
            .find(|provider| provider.descriptor().id == id)
            .map(Box::as_ref)
    }

    /// The provider's descriptor with `id`.
    #[must_use]
    pub fn descriptor(&self, id: ProviderId) -> Option<&'static ProviderDescriptor> {
        self.get(id).map(|provider| provider.descriptor())
    }

    /// All providers, in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Transcriber> {
        self.providers.iter().map(Box::as_ref)
    }

    /// The number of registered providers.
    #[must_use]
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// Whether no providers are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ModelOption, Readiness, TranscribeError, TranscriptionRequest};
    use crate::transcript::TranscriptDraft;

    struct DummyTranscriber {
        descriptor: &'static ProviderDescriptor,
    }

    impl Transcriber for DummyTranscriber {
        fn descriptor(&self) -> &'static ProviderDescriptor {
            self.descriptor
        }

        fn readiness(&self, api_key_present: bool) -> Readiness {
            if api_key_present {
                Readiness::Ready
            } else {
                Readiness::NeedsApiKey
            }
        }

        fn transcribe(
            &self,
            _request: &TranscriptionRequest<'_>,
            _observer: &dyn crate::ports::TranscriptionObserver,
        ) -> Result<TranscriptDraft, TranscribeError> {
            Err(TranscribeError::NoSpeech)
        }
    }

    const MODELS: &[ModelOption] = &[];

    const fn descriptor_for(id: ProviderId) -> ProviderDescriptor {
        ProviderDescriptor {
            id,
            display_name: "Dummy",
            requires_api_key: false,
            models: MODELS,
            model_default: "dummy",
            audio_bytes_max: 1,
            audio_duration_ms_max: 1,
        }
    }

    static APPLE_DESC: ProviderDescriptor = descriptor_for(ProviderId::AppleSpeech);
    static VOXTRAL_DESC: ProviderDescriptor = descriptor_for(ProviderId::MistralVoxtral);
    static DEEPGRAM_DESC: ProviderDescriptor = descriptor_for(ProviderId::Deepgram);

    #[test]
    #[should_panic]
    fn missing_provider_fails() {
        let _ = ProviderRegistry::new(vec![Box::new(DummyTranscriber {
            descriptor: &APPLE_DESC,
        })]);
    }

    #[test]
    #[should_panic]
    fn duplicate_provider_fails() {
        let _ = ProviderRegistry::new(vec![
            Box::new(DummyTranscriber {
                descriptor: &APPLE_DESC,
            }),
            Box::new(DummyTranscriber {
                descriptor: &APPLE_DESC,
            }),
        ]);
    }

    #[test]
    fn get_returns_the_provider() {
        let registry = ProviderRegistry::new(vec![
            Box::new(DummyTranscriber {
                descriptor: &APPLE_DESC,
            }),
            Box::new(DummyTranscriber {
                descriptor: &VOXTRAL_DESC,
            }),
            Box::new(DummyTranscriber {
                descriptor: &DEEPGRAM_DESC,
            }),
        ]);
        assert!(registry.get(ProviderId::AppleSpeech).is_some());
        assert!(registry.get(ProviderId::MistralVoxtral).is_some());
        assert!(registry.get(ProviderId::Deepgram).is_some());
        assert_eq!(registry.len(), 3);
        assert!(!registry.is_empty());
    }
}
