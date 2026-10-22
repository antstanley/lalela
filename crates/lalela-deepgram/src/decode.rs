//! Decoding and normalising Deepgram responses (02-transcription-providers.md).

use lalela_core::limits::MILLISECONDS_MAX_U64;
use lalela_core::ports::TranscribeError;
use lalela_core::transcript::{Segment, TranscriptDraft};
use lalela_core::units::{LanguageTag, Milliseconds};
use serde::Deserialize;

/// The response DTO.
#[derive(Debug, Deserialize)]
pub struct ResponseDto {
    /// The results object.
    #[serde(default)]
    pub results: Option<ResultsDto>,
}

/// Top-level results.
#[derive(Debug, Deserialize)]
pub struct ResultsDto {
    /// One channel per audio track; Lalela uses the first.
    #[serde(default)]
    pub channels: Vec<ChannelDto>,
}

/// One channel.
#[derive(Debug, Deserialize)]
pub struct ChannelDto {
    /// The alternatives, best first.
    #[serde(default)]
    pub alternatives: Vec<AlternativeDto>,
    /// The detected language, when detection ran.
    pub detected_language: Option<String>,
}

/// One alternative.
#[derive(Debug, Deserialize)]
pub struct AlternativeDto {
    /// Paragraphs, when `paragraphs=true` was requested.
    pub paragraphs: Option<ParagraphsDto>,
}

/// The paragraphs container.
#[derive(Debug, Deserialize)]
pub struct ParagraphsDto {
    /// The paragraphs.
    #[serde(default)]
    pub paragraphs: Vec<ParagraphDto>,
}

/// One paragraph.
#[derive(Debug, Deserialize)]
pub struct ParagraphDto {
    /// The paragraph's sentences.
    #[serde(default)]
    pub sentences: Vec<SentenceDto>,
}

/// One sentence; becomes one segment.
#[derive(Debug, Deserialize)]
pub struct SentenceDto {
    /// The sentence text.
    pub text: String,
    /// Start time in seconds.
    pub start: f64,
    /// End time in seconds.
    pub end: f64,
}

/// Decode a Deepgram response body into a draft. The requested language tag is
/// used as the fallback when detection did not run.
pub fn decode_response(
    bytes: &[u8],
    requested_language: Option<&LanguageTag>,
) -> Result<TranscriptDraft, TranscribeError> {
    let dto: ResponseDto =
        serde_json::from_slice(bytes).map_err(|error| TranscribeError::MalformedResponse {
            detail: format!("response decode failed: {error}"),
        })?;

    let Some(channel) = dto
        .results
        .and_then(|results| results.channels.into_iter().next())
    else {
        return Err(TranscribeError::MalformedResponse {
            detail: "response has no channels".to_owned(),
        });
    };
    let Some(alternative) = channel.alternatives.into_iter().next() else {
        return Err(TranscribeError::MalformedResponse {
            detail: "channel has no alternatives".to_owned(),
        });
    };
    let Some(paragraphs) = alternative.paragraphs else {
        return Err(TranscribeError::MalformedResponse {
            detail: "response has no paragraphs (paragraphs=true was requested)".to_owned(),
        });
    };

    let mut normalised = Vec::new();
    for paragraph in paragraphs.paragraphs {
        for sentence in paragraph.sentences {
            let text = crate::decode::normalise_text(&sentence.text);
            if text.is_empty() {
                continue;
            }
            let start_ms = seconds_to_ms(sentence.start)?;
            let end_ms = seconds_to_ms(sentence.end)?;
            normalised.push(Segment {
                start_ms,
                end_ms,
                text,
            });
        }
    }

    if normalised.is_empty() {
        return Err(TranscribeError::NoSpeech);
    }

    let language = channel
        .detected_language
        .and_then(LanguageTag::try_new)
        .or_else(|| requested_language.cloned());

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

    const SAMPLE: &[u8] = br#"{
      "metadata": { "request_id": "x", "model_info": {} },
      "results": {
        "channels": [
          {
            "alternatives": [
              {
                "transcript": "Welcome back.",
                "confidence": 0.99,
                "paragraphs": {
                  "transcript": "Welcome back.",
                  "paragraphs": [
                    {
                      "sentences": [
                        { "text": "Welcome back.", "start": 0.0, "end": 2.14, "confidence": 0.99 }
                      ],
                      "start": 0.0,
                      "end": 2.14
                    }
                  ]
                }
              }
            ],
            "detected_language": "en-GB"
          }
        ]
      }
    }"#;

    #[test]
    fn decodes_sample() {
        let draft = decode_response(SAMPLE, None).unwrap();
        assert_eq!(draft.segments.len(), 1);
        assert_eq!(draft.segments[0].text, "Welcome back.");
        assert_eq!(draft.segments[0].end_ms.as_u64(), 2140);
        assert_eq!(draft.language.unwrap().as_str(), "en-GB");
    }

    #[test]
    fn missing_paragraphs_is_malformed() {
        let json =
            br#"{ "results": { "channels": [ { "alternatives": [ { "transcript": "x" } ] } ] } }"#;
        assert!(matches!(
            decode_response(json, None),
            Err(TranscribeError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn empty_channels_is_malformed() {
        let json = br#"{ "results": { "channels": [] } }"#;
        assert!(matches!(
            decode_response(json, None),
            Err(TranscribeError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn empty_sentences_is_no_speech() {
        let json = br#"{
          "results": { "channels": [ { "alternatives": [ { "paragraphs": { "paragraphs": [ { "sentences": [ { "text": "  ", "start": 0.0, "end": 1.0 } ] } ] } } ] } ] }
        }"#;
        assert!(matches!(
            decode_response(json, None),
            Err(TranscribeError::NoSpeech)
        ));
    }

    #[test]
    fn invalid_json_is_malformed() {
        assert!(matches!(
            decode_response(b"not json", None),
            Err(TranscribeError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn requested_language_is_fallback() {
        let tag = LanguageTag::try_new("fr".to_owned()).unwrap();
        let draft = decode_response(SAMPLE, Some(&tag)).unwrap();
        // detected_language wins.
        assert_eq!(draft.language.unwrap().as_str(), "en-GB");
    }

    #[test]
    fn negative_time_is_malformed() {
        let json = br#"{
          "results": { "channels": [ { "alternatives": [ { "paragraphs": { "paragraphs": [ { "sentences": [ { "text": "x", "start": -0.1, "end": 1.0 } ] } ] } } ] } ] }
        }"#;
        assert!(matches!(
            decode_response(json, None),
            Err(TranscribeError::MalformedResponse { .. })
        ));
    }
}
