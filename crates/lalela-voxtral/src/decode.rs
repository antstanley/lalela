//! Decoding and normalising Voxtral responses (02-transcription-providers.md).

use lalela_core::limits::MILLISECONDS_MAX_U64;
use lalela_core::ports::TranscribeError;
use lalela_core::transcript::{Segment, TranscriptDraft};
use lalela_core::units::{LanguageTag, Milliseconds};
use serde::Deserialize;

/// The response DTO. Unknown fields (for example `usage`) are ignored.
#[derive(Debug, Deserialize)]
pub struct ResponseDto {
    /// The language the provider used or detected.
    pub language: Option<String>,
    /// The plain text.
    pub text: Option<String>,
    /// The model that produced the response.
    pub model: Option<String>,
    /// Timed segments.
    #[serde(default)]
    pub segments: Option<Vec<SegmentDto>>,
}

/// One timed segment.
#[derive(Debug, Deserialize)]
pub struct SegmentDto {
    /// The segment text.
    pub text: String,
    /// Start time in seconds.
    pub start: f64,
    /// End time in seconds.
    pub end: f64,
}

/// Decode a Voxtral response body into a draft.
pub fn decode_response(bytes: &[u8]) -> Result<TranscriptDraft, TranscribeError> {
    let dto: ResponseDto =
        serde_json::from_slice(bytes).map_err(|error| TranscribeError::MalformedResponse {
            detail: format!("response decode failed: {error}"),
        })?;

    let Some(segments) = dto.segments else {
        return Err(TranscribeError::MalformedResponse {
            detail: "response has no segments array".to_owned(),
        });
    };

    let mut normalised = Vec::with_capacity(segments.len());
    for segment in segments {
        let text = normalise_text(&segment.text);
        if text.is_empty() {
            continue;
        }
        let start_ms = seconds_to_ms(segment.start)?;
        let end_ms = seconds_to_ms(segment.end)?;
        normalised.push(Segment {
            start_ms,
            end_ms,
            text,
        });
    }

    if normalised.is_empty() {
        return Err(TranscribeError::NoSpeech);
    }

    let language = dto.language.and_then(LanguageTag::try_new);

    Ok(TranscriptDraft {
        segments: normalised,
        language,
    })
}

/// Trim text, replace internal line breaks with a space, and collapse runs of
/// whitespace.
pub fn normalise_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Convert seconds to whole milliseconds: `(seconds * 1000.0).round()` after
/// asserting the value is finite and non-negative.
fn seconds_to_ms(seconds: f64) -> Result<Milliseconds, TranscribeError> {
    if !seconds.is_finite() || seconds < 0.0 {
        return Err(TranscribeError::MalformedResponse {
            detail: "segment time is not a finite non-negative number".to_owned(),
        });
    }
    let ms = (seconds * 1000.0).round();
    if ms > MILLISECONDS_MAX_U64 as f64 {
        return Err(TranscribeError::MalformedResponse {
            detail: "segment time exceeds the media bound".to_owned(),
        });
    }
    Milliseconds::try_new(ms as u64).ok_or_else(|| TranscribeError::MalformedResponse {
        detail: "segment time out of range".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_valid_response() {
        let json = br#"{
            "text": "Welcome back.\nToday we're talking.",
            "language": "en",
            "model": "voxtral-mini-2602",
            "segments": [
                { "text": "Welcome back.", "start": 0.0, "end": 2.14 },
                { "text": "Today we're talking.", "start": 2.48, "end": 5.31 }
            ],
            "usage": { "total_tokens": 10 }
        }"#;
        let draft = decode_response(json).unwrap();
        assert_eq!(draft.segments.len(), 2);
        assert_eq!(draft.segments[0].start_ms.as_u64(), 0);
        assert_eq!(draft.segments[0].end_ms.as_u64(), 2140);
        assert_eq!(draft.segments[1].start_ms.as_u64(), 2480);
        assert_eq!(draft.segments[1].end_ms.as_u64(), 5310);
        assert_eq!(draft.language.unwrap().as_str(), "en");
    }

    #[test]
    fn missing_segments_is_malformed() {
        let json = br#"{ "text": "hi" }"#;
        assert!(matches!(
            decode_response(json),
            Err(TranscribeError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn empty_segments_is_no_speech() {
        let json = br#"{ "segments": [ { "text": "  ", "start": 0.0, "end": 1.0 } ] }"#;
        assert!(matches!(
            decode_response(json),
            Err(TranscribeError::NoSpeech)
        ));
    }

    #[test]
    fn invalid_json_is_malformed() {
        assert!(matches!(
            decode_response(b"not json"),
            Err(TranscribeError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn negative_time_is_malformed() {
        let json = br#"{ "segments": [ { "text": "hi", "start": -1.0, "end": 1.0 } ] }"#;
        assert!(matches!(
            decode_response(json),
            Err(TranscribeError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn internal_newlines_become_spaces() {
        assert_eq!(normalise_text("a\nb\r\nc"), "a b c");
        assert_eq!(normalise_text("  padded  "), "padded");
    }

    #[test]
    fn segments_out_of_order_survive_decode() {
        // Ordering is checked later by Transcript::from_draft; decode only
        // normalises.
        let json = br#"{ "segments": [ { "text": "b", "start": 2.0, "end": 3.0 }, { "text": "a", "start": 1.0, "end": 2.0 } ] }"#;
        let draft = decode_response(json).unwrap();
        assert_eq!(draft.segments.len(), 2);
    }
}
