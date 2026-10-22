//! Deepgram transcription adapter (02-transcription-providers.md).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod decode;

use lalela_core::limits::{AUDIO_BYTES_MAX, AUDIO_DURATION_MS_MAX};
use lalela_core::ports::{
    ModelOption, Progress, ProviderDescriptor, ProviderId, Readiness, TranscribeError, Transcriber,
    TranscriptionObserver, TranscriptionRequest,
};
use lalela_core::transcript::TranscriptDraft;
use lalela_core::units::{Bytes, LanguagePreference};
use lalela_http::{HttpClient, HttpError, HttpRequest};

/// The transcription endpoint.
pub const DEEPGRAM_ENDPOINT: &str = "https://api.deepgram.com/v1/listen";

const MODELS: &[ModelOption] = &[
    ModelOption {
        id: "nova-3",
        label: "Nova 3",
        supports_language_hint: true,
    },
    ModelOption {
        id: "nova-2",
        label: "Nova 2",
        supports_language_hint: true,
    },
];

static DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: ProviderId::Deepgram,
    display_name: "Deepgram",
    requires_api_key: true,
    models: MODELS,
    model_default: "nova-3",
    audio_bytes_max: AUDIO_BYTES_MAX,
    audio_duration_ms_max: AUDIO_DURATION_MS_MAX,
};

/// The Deepgram adapter.
pub struct DeepgramTranscriber {
    http: HttpClient,
}

impl DeepgramTranscriber {
    /// Build the adapter with a shared HTTP client.
    #[must_use]
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }
}

impl Transcriber for DeepgramTranscriber {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        &DESCRIPTOR
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
        request: &TranscriptionRequest<'_>,
        observer: &dyn TranscriptionObserver,
    ) -> Result<TranscriptDraft, TranscribeError> {
        let descriptor = self.descriptor();
        assert!(
            request.audio.size_bytes.as_u64() <= descriptor.audio_bytes_max,
            "audio exceeds the provider's byte cap"
        );
        assert!(
            request.audio.duration_ms.as_u64() <= descriptor.audio_duration_ms_max,
            "audio exceeds the provider's duration cap"
        );
        assert!(
            descriptor
                .models
                .iter()
                .any(|model| std::ptr::eq(model, request.model)),
            "model does not belong to this provider"
        );
        assert!(
            !descriptor.requires_api_key || request.api_key.is_some(),
            "api key missing"
        );

        let audio_bytes =
            std::fs::read(&request.audio.path).map_err(|_| TranscribeError::FileUnreadable)?;
        let http_request = build_request(request, audio_bytes)?;

        let is_cancelled = || observer.is_cancelled();
        let on_progress = |sent: u64, total: u64| {
            observer.on_progress(Progress::Uploading {
                sent: Bytes::new(sent),
                total: Bytes::new(total),
            });
        };

        let response = self.http.send(&http_request, &is_cancelled, &on_progress);
        observer.on_progress(Progress::Processing { permille: None });
        let response = response.map_err(map_http_error)?;
        crate::decode::decode_response(&response.body, request.language.tag())
    }
}

/// Build the raw-bytes request with query parameters.
fn build_request(
    request: &TranscriptionRequest<'_>,
    audio_bytes: Vec<u8>,
) -> Result<HttpRequest, TranscribeError> {
    let mut url = String::with_capacity(128);
    url.push_str(DEEPGRAM_ENDPOINT);
    url.push_str("?model=");
    url.push_str(request.model.id);
    url.push_str("&smart_format=true&paragraphs=true");
    match request.language {
        LanguagePreference::Auto => {
            url.push_str("&detect_language=true");
        }
        LanguagePreference::Tag(tag) => {
            url.push_str("&language=");
            url.push_str(tag.as_str());
        }
    }

    Ok(HttpRequest {
        url,
        auth: request
            .api_key
            .map(|key| format!("Token {}", key.as_secret())),
        headers: vec![],
        body: lalela_http::HttpBody::Bytes {
            content_type: request.audio.format.mime().to_owned(),
            bytes: audio_bytes,
        },
    })
}

/// Translate an HTTP failure into a `TranscribeError`.
fn map_http_error(error: HttpError) -> TranscribeError {
    match error {
        HttpError::Cancelled => TranscribeError::Cancelled,
        HttpError::Timeout => TranscribeError::Timeout,
        HttpError::Network { detail } => TranscribeError::Network { detail },
        HttpError::ResponseTooLarge => TranscribeError::MalformedResponse {
            detail: "response exceeded HTTP_RESPONSE_BYTES_MAX".to_owned(),
        },
        HttpError::InvalidUrl { detail } => TranscribeError::MalformedResponse { detail },
        HttpError::Status {
            status, excerpt, ..
        } => match status {
            401 | 403 => TranscribeError::Unauthorized,
            429 => TranscribeError::RateLimited,
            413 => TranscribeError::PayloadTooLarge,
            415 => TranscribeError::UnsupportedFormat,
            400 => TranscribeError::UnsupportedLanguage,
            404 => TranscribeError::ModelUnavailable,
            504 => TranscribeError::Timeout,
            _ => TranscribeError::Provider {
                status,
                message: excerpt.unwrap_or_default(),
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lalela_core::audio::AudioSource;
    use lalela_core::id::TranscriptId;
    use lalela_core::units::ApiKey;
    use lalela_core::units::LanguageTag;
    use lalela_core::units::Milliseconds;

    fn sample_request() -> TranscriptionRequest<'static> {
        let id = Box::leak(Box::new(
            TranscriptId::try_new("trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f".to_owned()).unwrap(),
        ));
        let audio = Box::leak(Box::new(
            AudioSource::try_new(
                std::path::PathBuf::from("/tmp/audio.m4a"),
                lalela_core::audio::AudioFormat::M4a,
                Bytes::new(1000),
                Milliseconds::try_new(60_000).unwrap(),
            )
            .unwrap(),
        ));
        let key = Box::leak(Box::new(
            ApiKey::try_new("0123456789abcdef".to_owned()).unwrap(),
        ));
        TranscriptionRequest {
            transcript_id: id,
            audio,
            model: &MODELS[0],
            language: &LanguagePreference::Auto,
            api_key: Some(key),
        }
    }

    #[test]
    fn descriptor_shape() {
        assert_eq!(DESCRIPTOR.id, ProviderId::Deepgram);
        assert!(DESCRIPTOR.requires_api_key);
        assert_eq!(DESCRIPTOR.model_default, "nova-3");
        assert_eq!(DESCRIPTOR.models.len(), 2);
    }

    #[test]
    fn build_request_auto_language() {
        let request = sample_request();
        let http_request = build_request(&request, vec![1, 2]).unwrap();
        assert!(http_request.url.contains("?model=nova-3"));
        assert!(http_request.url.contains("smart_format=true"));
        assert!(http_request.url.contains("paragraphs=true"));
        assert!(http_request.url.contains("detect_language=true"));
        assert!(!http_request.url.contains("&language="));
        assert_eq!(http_request.auth.as_deref(), Some("Token 0123456789abcdef"));
    }

    #[test]
    fn build_request_explicit_language() {
        let request = sample_request();
        let request = TranscriptionRequest {
            language: &LanguagePreference::Tag(LanguageTag::try_new("en-GB".to_owned()).unwrap()),
            ..request
        };
        let http_request = build_request(&request, vec![]).unwrap();
        assert!(http_request.url.contains("&language=en-GB"));
        assert!(!http_request.url.contains("detect_language"));
    }

    #[test]
    fn readiness_needs_key() {
        let adapter = DeepgramTranscriber::new(HttpClient::new());
        assert_eq!(adapter.readiness(false), Readiness::NeedsApiKey);
        assert_eq!(adapter.readiness(true), Readiness::Ready);
    }
}
