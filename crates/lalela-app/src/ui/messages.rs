//! UI messages for port errors (architecture-principles.md §Errors).

use lalela_core::ports::TranscribeError;

/// One sentence for a transcription error.
#[must_use]
pub fn message_for_transcribe_error(error: &TranscribeError) -> String {
    match error {
        TranscribeError::Cancelled => "Transcription cancelled.".to_owned(),
        TranscribeError::FileUnreadable => "Lalela could not read the audio file.".to_owned(),
        TranscribeError::ProviderUnavailable { reason } => {
            format!("The provider is not available: {reason}.")
        }
        TranscribeError::Unauthorized => {
            "The provider rejected the API key. Update it in Settings.".to_owned()
        }
        TranscribeError::RateLimited => "The provider is rate-limiting requests.".to_owned(),
        TranscribeError::PayloadTooLarge => "File is too large for this provider.".to_owned(),
        TranscribeError::UnsupportedFormat => {
            "The provider cannot read this file format.".to_owned()
        }
        TranscribeError::UnsupportedLanguage => {
            "The provider does not support this language.".to_owned()
        }
        TranscribeError::ModelUnavailable => "The selected model is not available.".to_owned(),
        TranscribeError::NoSpeech => "No speech was found in this file.".to_owned(),
        TranscribeError::Timeout => "The provider timed out.".to_owned(),
        TranscribeError::Network { .. } => "Could not reach the provider.".to_owned(),
        TranscribeError::Provider { message, .. } if !message.is_empty() => {
            format!("The provider returned an error: {message}")
        }
        TranscribeError::Provider { .. } => "The provider returned an error.".to_owned(),
        TranscribeError::MalformedResponse { .. } => {
            "The provider returned an unexpected response.".to_owned()
        }
        TranscribeError::PlatformFailure { .. } => "An internal macOS error occurred.".to_owned(),
    }
}
