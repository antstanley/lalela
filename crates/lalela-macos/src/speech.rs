//! `AppleSpeechTranscriber`: the Apple on-device provider
//! (02-transcription-providers.md §Apple on-device Speech).

use std::ffi::{CString, c_void};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use lalela_core::limits::{MILLISECONDS_MAX_U64, TRANSCRIPT_TEXT_BYTES_MAX};
use lalela_core::ports::{
    ModelOption, ProviderDescriptor, ProviderId, Readiness, TranscribeError, Transcriber,
    TranscriptionObserver, TranscriptionRequest,
};
use lalela_core::transcript::{Segment, TranscriptDraft};
use lalela_core::units::{LanguageTag, Milliseconds};
use serde::Deserialize;

/// The shim speech output buffer capacity (08-limits.md §Shim).
pub const SHIM_SPEECH_OUTPUT_BYTES_MAX: u32 = 16 * 1024 * 1024;
/// The speech export timeout (08-limits.md §Shim).
pub const SPEECH_TIMEOUT_MS: u64 = 3_600_000;

const _: () = assert!(
    SHIM_SPEECH_OUTPUT_BYTES_MAX as u64 >= TRANSCRIPT_TEXT_BYTES_MAX as u64 + 20_000 * 128,
    "shim buffer must hold a maximum-size transcript"
);

const MODELS: &[ModelOption] = &[ModelOption {
    id: "speech-transcriber",
    label: "On-device",
    supports_language_hint: true,
}];

static DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: ProviderId::AppleSpeech,
    display_name: "Apple (on-device)",
    requires_api_key: false,
    models: MODELS,
    model_default: "speech-transcriber",
    audio_bytes_max: lalela_core::limits::AUDIO_BYTES_MAX,
    audio_duration_ms_max: lalela_core::limits::AUDIO_DURATION_MS_MAX,
};

/// The shim's speech output JSON.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShimSpeechOutput {
    /// The locale the shim used, in BCP-47 form.
    pub locale: String,
    /// The segments.
    pub segments: Vec<ShimSpeechSegment>,
}

/// One shim segment.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShimSpeechSegment {
    /// Start in milliseconds.
    pub start_ms: u64,
    /// End in milliseconds.
    pub end_ms: u64,
    /// The segment text.
    pub text: String,
}

/// The pointer handed to the shim as `context`; borrowed for the call.
struct SpeechCallContext<'a> {
    observer: &'a dyn TranscriptionObserver,
    cancelled: Arc<AtomicBool>,
}

extern "C" fn on_progress_cb(context: *mut c_void, phase: u32, permille: u32) {
    let result = std::panic::catch_unwind(|| {
        let ctx = unsafe { &*(context as *const SpeechCallContext<'_>) };
        let progress = match phase {
            1 => {
                let permille = permille.min(1000) as u16;
                lalela_core::ports::Progress::PreparingModel { permille }
            }
            2 => {
                let permille = permille.min(1000) as u16;
                lalela_core::ports::Progress::Processing {
                    permille: Some(permille),
                }
            }
            _ => return,
        };
        ctx.observer.on_progress(progress);
    });
    if result.is_err() {
        // Never let a panic unwind across the FFI boundary.
        std::process::abort();
    }
}

extern "C" fn is_cancelled_cb(context: *mut c_void) -> bool {
    let result = std::panic::catch_unwind(|| {
        let ctx = unsafe { &*(context as *const SpeechCallContext<'_>) };
        ctx.cancelled.load(Ordering::Relaxed) || ctx.observer.is_cancelled()
    });
    match result {
        Ok(value) => value,
        Err(_) => {
            std::process::abort();
        }
    }
}

/// The Apple on-device provider.
pub struct AppleSpeechTranscriber {
    available: bool,
}

impl AppleSpeechTranscriber {
    /// Probe availability once; readiness never crosses the FFI boundary per
    /// frame again.
    #[must_use]
    pub fn new() -> Self {
        let available = unsafe { crate::ffi::lalela_speech_is_available() };
        Self { available }
    }
}

impl Default for AppleSpeechTranscriber {
    fn default() -> Self {
        Self::new()
    }
}

impl Transcriber for AppleSpeechTranscriber {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        &DESCRIPTOR
    }

    fn readiness(&self, _api_key_present: bool) -> Readiness {
        if self.available {
            Readiness::Ready
        } else {
            Readiness::Unavailable {
                reason: "on-device speech is not supported on this Mac",
            }
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

        let audio_path = CString::new(request.audio.path.as_os_str().as_encoded_bytes())
            .map_err(|_| TranscribeError::FileUnreadable)?;
        let locale = match request.language.tag() {
            None => c"".to_owned(),
            Some(tag) => {
                CString::new(tag.as_str()).map_err(|_| TranscribeError::UnsupportedLanguage)?
            }
        };

        let cancelled = Arc::new(AtomicBool::new(false));
        let context = SpeechCallContext {
            observer,
            cancelled: cancelled.clone(),
        };
        let context_ptr = &context as *const SpeechCallContext<'_> as *mut c_void;

        let mut out_json = vec![0u8; SHIM_SPEECH_OUTPUT_BYTES_MAX as usize];
        let mut out_len: u32 = 0;

        // SAFETY: `audio_path` and `locale` are valid NUL-terminated C strings
        // for the duration of the call; `context_ptr` borrows `context`, which
        // outlives the call; `out_json` is a caller-owned buffer with its
        // capacity passed explicitly; the shim's `abandoned` lock guarantees no
        // callback runs after the export returns.
        let status = unsafe {
            crate::ffi::lalela_speech_transcribe(
                audio_path.as_ptr(),
                locale.as_ptr(),
                SPEECH_TIMEOUT_MS,
                context_ptr,
                on_progress_cb,
                is_cancelled_cb,
                out_json.as_mut_ptr(),
                SHIM_SPEECH_OUTPUT_BYTES_MAX,
                &mut out_len,
            )
        };
        let status = crate::ffi::ShimStatus::from_raw(status);

        match status {
            crate::ffi::ShimStatus::Ok => {
                assert!(
                    out_len as usize <= out_json.len(),
                    "shim reported an output length beyond the buffer"
                );
                let output =
                    serde_json::from_slice::<ShimSpeechOutput>(&out_json[..out_len as usize])
                        .map_err(|error| TranscribeError::MalformedResponse {
                            detail: format!("shim output decode failed: {error}"),
                        })?;
                self.draft_from_output(output, request.audio.duration_ms)
            }
            crate::ffi::ShimStatus::InvalidArgument => {
                // Rust validated every argument and calls from the worker
                // thread, so this status means a Lalela bug.
                panic!("shim reported InvalidArgument for a valid speech call")
            }
            crate::ffi::ShimStatus::FileNotFound => Err(TranscribeError::FileUnreadable),
            crate::ffi::ShimStatus::UnsupportedAudio => Err(TranscribeError::UnsupportedFormat),
            crate::ffi::ShimStatus::SpeechUnavailable => {
                Err(TranscribeError::ProviderUnavailable {
                    reason: "on-device speech is not supported on this Mac".to_owned(),
                })
            }
            crate::ffi::ShimStatus::LocaleUnsupported => Err(TranscribeError::UnsupportedLanguage),
            crate::ffi::ShimStatus::AssetsUnavailable => Err(TranscribeError::ModelUnavailable),
            crate::ffi::ShimStatus::Cancelled => Err(TranscribeError::Cancelled),
            crate::ffi::ShimStatus::TimedOut => Err(TranscribeError::Timeout),
            crate::ffi::ShimStatus::OutputTooLarge => Err(TranscribeError::MalformedResponse {
                detail: "shim output exceeded SHIM_SPEECH_OUTPUT_BYTES_MAX".to_owned(),
            }),
            crate::ffi::ShimStatus::NoPlayer => {
                panic!("shim reported NoPlayer from the speech export")
            }
            crate::ffi::ShimStatus::InsufficientResources => {
                Err(TranscribeError::ProviderUnavailable {
                    reason: "the system is busy with other speech analysis".to_owned(),
                })
            }
            crate::ffi::ShimStatus::Internal => {
                Err(TranscribeError::PlatformFailure { status: 99 })
            }
            crate::ffi::ShimStatus::Unknown(status) => {
                Err(TranscribeError::PlatformFailure { status })
            }
        }
    }
}

impl AppleSpeechTranscriber {
    /// Build a draft from the shim's validated output.
    fn draft_from_output(
        &self,
        output: ShimSpeechOutput,
        audio_duration: Milliseconds,
    ) -> Result<TranscriptDraft, TranscribeError> {
        let mut segments = Vec::with_capacity(output.segments.len());
        for segment in output.segments {
            let start_ms = Milliseconds::try_new(segment.start_ms).ok_or_else(|| {
                TranscribeError::MalformedResponse {
                    detail: "segment start out of range".to_owned(),
                }
            })?;
            let end_ms = Milliseconds::try_new(segment.end_ms).ok_or_else(|| {
                TranscribeError::MalformedResponse {
                    detail: "segment end out of range".to_owned(),
                }
            })?;
            let text = segment
                .text
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            if text.is_empty() {
                continue;
            }
            segments.push(Segment {
                start_ms,
                end_ms,
                text,
            });
        }
        if segments.is_empty() {
            return Err(TranscribeError::NoSpeech);
        }
        let language = if output.locale.is_empty() {
            None
        } else {
            LanguageTag::try_new(output.locale)
        };
        let _ = audio_duration;
        let _ = MILLISECONDS_MAX_U64;
        Ok(TranscriptDraft { segments, language })
    }
}
