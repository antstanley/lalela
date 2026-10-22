//! Mistral Voxtral transcription adapter (02-transcription-providers.md).
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod decode;

use lalela_core::audio::AudioSource;
use lalela_core::ports::{
    ModelOption, Progress, ProviderDescriptor, ProviderId, Readiness, TranscribeError, Transcriber,
    TranscriptionObserver, TranscriptionRequest,
};
use lalela_core::transcript::TranscriptDraft;
use lalela_core::units::Bytes;
use lalela_http::{HttpClient, HttpError, HttpRequest, MultipartPart};

/// Voxtral direct-upload byte cap (placeholder until Mistral documents it).
pub const VOXTRAL_AUDIO_BYTES_MAX: u64 = 104_857_600;
/// Voxtral audio duration cap, in milliseconds.
pub const VOXTRAL_AUDIO_DURATION_MS_MAX: u64 = 10_800_000;

/// The transcription endpoint.
pub const VOXTRAL_ENDPOINT: &str = "https://api.mistral.ai/v1/audio/transcriptions";

const MODELS: &[ModelOption] = &[
    ModelOption {
        id: "voxtral-mini-2602",
        label: "Voxtral Mini 2 (2602)",
        supports_language_hint: false,
    },
    ModelOption {
        id: "voxtral-mini-transcribe-3-0",
        label: "Voxtral Mini Transcribe 3",
        supports_language_hint: true,
    },
    ModelOption {
        id: "voxtral-small-transcribe-3-0",
        label: "Voxtral Small Transcribe 3",
        supports_language_hint: true,
    },
];

static DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: ProviderId::MistralVoxtral,
    display_name: "Mistral Voxtral",
    requires_api_key: true,
    models: MODELS,
    model_default: "voxtral-mini-2602",
    audio_bytes_max: VOXTRAL_AUDIO_BYTES_MAX,
    audio_duration_ms_max: VOXTRAL_AUDIO_DURATION_MS_MAX,
};

/// The Voxtral adapter.
pub struct VoxtralTranscriber {
    http: HttpClient,
}

impl VoxtralTranscriber {
    /// Build the adapter with a shared HTTP client.
    #[must_use]
    pub fn new(http: HttpClient) -> Self {
        Self { http }
    }

    /// Read the audio file bytes for upload.
    fn read_audio_bytes(&self, audio: &AudioSource) -> Result<Vec<u8>, TranscribeError> {
        std::fs::read(&audio.path).map_err(|_| TranscribeError::FileUnreadable)
    }
}

impl Transcriber for VoxtralTranscriber {
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

        let sent_language =
            request.language.tag().is_some() && request.model.supports_language_hint;
        let audio_bytes = self.read_audio_bytes(request.audio)?;
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
        let response = response.map_err(|error| map_http_error(error, sent_language))?;
        crate::decode::decode_response(&response.body)
    }
}

/// Build the multipart request.
fn build_request(
    request: &TranscriptionRequest<'_>,
    audio_bytes: Vec<u8>,
) -> Result<HttpRequest, TranscribeError> {
    let boundary = format!("lalela-{}", request.transcript_id.uuid_hex());

    let mut parts = Vec::with_capacity(4);
    parts.push(MultipartPart {
        disposition: "form-data; name=\"model\"".to_owned(),
        headers: vec![],
        body: request.model.id.as_bytes().to_vec(),
    });
    parts.push(MultipartPart {
        disposition: "form-data; name=\"timestamp_granularities\"".to_owned(),
        headers: vec![],
        body: b"segment".to_vec(),
    });
    if let Some(tag) = request.language.tag() {
        assert!(
            request.model.supports_language_hint,
            "language part sent for a model without a language hint"
        );
        parts.push(MultipartPart {
            disposition: "form-data; name=\"language\"".to_owned(),
            headers: vec![],
            body: tag.primary_subtag().as_bytes().to_vec(),
        });
    }
    let disposition = format!(
        "form-data; name=\"file\"; filename=\"{}\"",
        request.audio.file_name
    );
    parts.push(MultipartPart {
        disposition,
        headers: vec![format!("Content-Type: {}", request.audio.format.mime())],
        body: audio_bytes,
    });

    Ok(HttpRequest {
        url: VOXTRAL_ENDPOINT.to_owned(),
        auth: request
            .api_key
            .map(|key| format!("Bearer {}", key.as_secret())),
        headers: vec![],
        body: lalela_http::HttpBody::Multipart { boundary, parts },
    })
}

/// Translate an HTTP failure into a `TranscribeError`.
fn map_http_error(error: HttpError, sent_language: bool) -> TranscribeError {
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
            400 if sent_language => TranscribeError::UnsupportedLanguage,
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
    use lalela_core::id::TranscriptId;
    use lalela_core::units::{ApiKey, LanguagePreference, Milliseconds, ModelName};

    #[test]
    fn descriptor_shape() {
        assert_eq!(DESCRIPTOR.id, ProviderId::MistralVoxtral);
        assert!(DESCRIPTOR.requires_api_key);
        assert_eq!(DESCRIPTOR.model_default, "voxtral-mini-2602");
        assert_eq!(DESCRIPTOR.models.len(), 3);
        assert!(!DESCRIPTOR.models[0].supports_language_hint);
        assert!(DESCRIPTOR.models[1].supports_language_hint);
        assert_eq!(DESCRIPTOR.audio_bytes_max, VOXTRAL_AUDIO_BYTES_MAX);
    }

    #[test]
    fn readiness_needs_key() {
        let adapter = VoxtralTranscriber::new(HttpClient::new());
        assert_eq!(adapter.readiness(false), Readiness::NeedsApiKey);
        assert_eq!(adapter.readiness(true), Readiness::Ready);
    }

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
    fn build_request_includes_parts() {
        let request = sample_request();
        let http_request = build_request(&request, vec![1, 2, 3]).unwrap();
        let lalela_http::HttpBody::Multipart { boundary, parts } = &http_request.body else {
            panic!("expected multipart body");
        };
        assert!(boundary.starts_with("lalela-"));
        assert_eq!(parts.len(), 3);
        assert!(parts[0].body == b"voxtral-mini-2602");
        assert!(parts[1].body == b"segment");
        assert!(parts[2].disposition.contains("filename=\"audio.m4a\""));
        assert_eq!(parts[2].body, vec![1, 2, 3]);
    }

    #[test]
    fn build_request_includes_language_when_supported() {
        let request = sample_request();
        let request = TranscriptionRequest {
            model: &MODELS[1],
            language: &LanguagePreference::Tag(
                lalela_core::units::LanguageTag::try_new("fr-FR".to_owned()).unwrap(),
            ),
            ..request
        };
        let http_request = build_request(&request, vec![]).unwrap();
        let lalela_http::HttpBody::Multipart { parts, .. } = &http_request.body else {
            panic!("expected multipart body");
        };
        assert_eq!(parts.len(), 4);
        assert!(parts[2].body == b"fr");
    }

    #[test]
    fn model_name_round_trip() {
        assert!(ModelName::try_new("voxtral-mini-2602".to_owned()).is_some());
    }
}
