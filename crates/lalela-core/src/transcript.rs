//! Transcription domain types: `Segment`, `TranscriptDraft`, and `Transcript`
//! (01-domain-model.md §Entities).

use crate::audio::AudioSource;
use crate::id::TranscriptId;
use crate::limits::{
    SEGMENT_END_TOLERANCE_MS, SEGMENT_TEXT_BYTES_MAX, SEGMENTS_COUNT_MAX, TRANSCRIPT_TEXT_BYTES_MAX,
};
use crate::ports::transcriber::{ProviderId, TranscribeError};
use crate::units::{LanguageTag, Milliseconds, ModelName};

/// One timed span of speech.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    /// Milliseconds from the start of the audio.
    pub start_ms: Milliseconds,
    /// Milliseconds from the start of the audio.
    pub end_ms: Milliseconds,
    /// The segment text, trimmed and without line breaks.
    pub text: String,
}

/// What a `Transcriber` returns: normalised segments without identity.
#[derive(Clone, Debug)]
pub struct TranscriptDraft {
    /// Segments in provider order.
    pub segments: Vec<Segment>,
    /// The language the provider used or detected, when it reports one.
    pub language: Option<LanguageTag>,
}

/// Why a `Transcript` could not be rebuilt from saved parts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartsError {
    /// There are no segments.
    NoSegments,
    /// More than `SEGMENTS_COUNT_MAX` segments.
    TooManySegments,
    /// A segment starts before the previous one.
    UnorderedSegments,
    /// A segment ends after the audio duration plus tolerance.
    EndBeyondAudio { end_ms: u64, duration_ms: u64 },
    /// A segment's text is empty after trimming.
    EmptyText,
    /// A segment's text contains a line break.
    NewlineInText,
    /// A segment's text exceeds `SEGMENT_TEXT_BYTES_MAX`.
    SegmentTextTooLong,
    /// The total text exceeds `TRANSCRIPT_TEXT_BYTES_MAX`.
    TotalTextTooLong,
    /// A structural problem (bad time order, malformed draft shape).
    Structural { detail: String },
}

/// A failure of transcript validation, distinguishing a malformed provider
/// response from a corrupt sidecar.
enum ValidateFailure {
    /// A malformed draft from a third party.
    Malformed(String),
    /// A corrupt saved document.
    Parts(PartsError),
}

/// A validated transcript of one audio file.
#[derive(Clone, Debug)]
pub struct Transcript {
    /// The transcript ID.
    pub id: TranscriptId,
    /// The provider that produced it.
    pub provider: ProviderId,
    /// The model id as sent to the provider.
    pub model: ModelName,
    /// The language, when known.
    pub language: Option<LanguageTag>,
    /// Creation time as Unix milliseconds.
    pub created_at_unix_ms: u64,
    /// The audio duration, copied from the `AudioSource`.
    pub audio_duration_ms: Milliseconds,
    /// The validated segments.
    pub segments: Vec<Segment>,
}

impl Transcript {
    /// Build a new transcript from a provider's draft. Runs the shared
    /// validation, clamping segment ends inside the tolerance to the audio
    /// duration and rejecting anything beyond it.
    pub fn from_draft(
        draft: TranscriptDraft,
        audio: &AudioSource,
        provider: ProviderId,
        model: ModelName,
        id: TranscriptId,
        created_at_unix_ms: u64,
    ) -> Result<Self, TranscribeError> {
        let duration = audio.duration_ms;
        let (segments, _) = match Self::validate(draft.segments, duration, true) {
            Ok(value) => value,
            Err(ValidateFailure::Malformed(detail)) => {
                return Err(TranscribeError::MalformedResponse { detail });
            }
            Err(ValidateFailure::Parts(error)) => {
                return Err(TranscribeError::MalformedResponse {
                    detail: format!("transcript invariant violated: {error:?}"),
                });
            }
        };
        Ok(Self {
            id,
            provider,
            model,
            language: draft.language,
            created_at_unix_ms,
            audio_duration_ms: duration,
            segments,
        })
    }

    /// Rebuild a transcript from decoded sidecar parts. The same invariants
    /// run again; ends inside the tolerance are accepted unchanged.
    pub fn from_parts(
        id: TranscriptId,
        provider: ProviderId,
        model: ModelName,
        language: Option<LanguageTag>,
        created_at_unix_ms: u64,
        audio_duration_ms: Milliseconds,
        segments: Vec<Segment>,
    ) -> Result<Self, PartsError> {
        let (segments, _) = match Self::validate(segments, audio_duration_ms, false) {
            Ok(value) => value,
            Err(ValidateFailure::Malformed(detail)) => {
                return Err(PartsError::Structural { detail });
            }
            Err(ValidateFailure::Parts(error)) => return Err(error),
        };
        Ok(Self {
            id,
            provider,
            model,
            language,
            created_at_unix_ms,
            audio_duration_ms,
            segments,
        })
    }

    /// The derived plain text: segment texts joined by `\n`. This is the only
    /// source of the `.txt` file and the sidecar's `text` field.
    #[must_use]
    pub fn text(&self) -> String {
        let mut text = String::new();
        for (index, segment) in self.segments.iter().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            text.push_str(&segment.text);
        }
        text
    }

    /// Validate the six transcript invariants. Returns the (possibly clamped)
    /// segments and whether any end was clamped.
    fn validate(
        segments: Vec<Segment>,
        duration: Milliseconds,
        clamp: bool,
    ) -> Result<(Vec<Segment>, bool), ValidateFailure> {
        if segments.is_empty() {
            return Err(ValidateFailure::Parts(PartsError::NoSegments));
        }
        if segments.len() > SEGMENTS_COUNT_MAX {
            return Err(ValidateFailure::Parts(PartsError::TooManySegments));
        }
        let duration_u64 = duration.as_u64();
        let mut validated = Vec::with_capacity(segments.len());
        let mut previous_start: Option<u64> = None;
        let mut total_bytes = 0usize;
        let mut clamped_any = false;
        for segment in segments {
            if segment.start_ms > segment.end_ms {
                return Err(ValidateFailure::Malformed(
                    "segment start after end".to_owned(),
                ));
            }
            if let Some(previous) = previous_start
                && segment.start_ms.as_u64() < previous
            {
                return Err(ValidateFailure::Parts(PartsError::UnorderedSegments));
            }
            let raw_end = segment.end_ms.as_u64();
            let beyond_tolerance = raw_end > duration_u64 + SEGMENT_END_TOLERANCE_MS;
            if raw_end > duration_u64 {
                if beyond_tolerance {
                    return Err(ValidateFailure::Parts(PartsError::EndBeyondAudio {
                        end_ms: raw_end,
                        duration_ms: duration_u64,
                    }));
                }
                if clamp {
                    clamped_any = true;
                }
            }
            let text = segment.text;
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(ValidateFailure::Parts(PartsError::EmptyText));
            }
            if trimmed.contains('\n') || trimmed.contains('\r') {
                return Err(ValidateFailure::Parts(PartsError::NewlineInText));
            }
            if trimmed.len() > SEGMENT_TEXT_BYTES_MAX {
                return Err(ValidateFailure::Parts(PartsError::SegmentTextTooLong));
            }
            total_bytes += trimmed.len();
            if total_bytes > TRANSCRIPT_TEXT_BYTES_MAX {
                return Err(ValidateFailure::Parts(PartsError::TotalTextTooLong));
            }
            let end = if clamp && raw_end > duration_u64 {
                duration
            } else {
                segment.end_ms
            };
            previous_start = Some(segment.start_ms.as_u64());
            validated.push(Segment {
                start_ms: segment.start_ms,
                end_ms: end,
                text: trimmed.to_owned(),
            });
        }
        Ok((validated, clamped_any))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioFormat, AudioSource};
    use crate::limits::{SEGMENTS_COUNT_MAX, TRANSCRIPT_TEXT_BYTES_MAX};

    fn audio(duration_ms: u64) -> AudioSource {
        AudioSource::try_new(
            std::path::PathBuf::from("/tmp/a.m4a"),
            AudioFormat::M4a,
            crate::units::Bytes::new(100),
            Milliseconds::try_new(duration_ms).unwrap(),
        )
        .unwrap()
    }

    fn id() -> TranscriptId {
        TranscriptId::try_new("trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f".to_owned()).unwrap()
    }

    fn model() -> ModelName {
        ModelName::try_new("nova-3".to_owned()).unwrap()
    }

    fn draft(segments: Vec<Segment>) -> TranscriptDraft {
        TranscriptDraft {
            segments,
            language: None,
        }
    }

    fn segment(start: u64, end: u64, text: &str) -> Segment {
        Segment {
            start_ms: Milliseconds::try_new(start).unwrap(),
            end_ms: Milliseconds::try_new(end).unwrap(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn from_draft_clamps_end_in_tolerance() {
        let source = audio(60_000);
        let draft = draft(vec![segment(0, 60_500, "hello")]);
        let transcript =
            Transcript::from_draft(draft, &source, ProviderId::Deepgram, model(), id(), 1).unwrap();
        assert_eq!(transcript.segments[0].end_ms, source.duration_ms);
    }

    #[test]
    fn from_draft_rejects_end_beyond_tolerance() {
        let source = audio(60_000);
        let draft = draft(vec![segment(0, 61_100, "hello")]);
        let result = Transcript::from_draft(draft, &source, ProviderId::Deepgram, model(), id(), 1);
        assert!(matches!(
            result,
            Err(TranscribeError::MalformedResponse { .. })
        ));
    }

    #[test]
    fn from_draft_rejects_out_of_order() {
        let source = audio(60_000);
        let draft = draft(vec![segment(1_000, 2_000, "b"), segment(0, 1_000, "a")]);
        assert!(
            Transcript::from_draft(draft, &source, ProviderId::Deepgram, model(), id(), 1).is_err()
        );
    }

    #[test]
    fn from_draft_rejects_empty_or_whitespace_text() {
        let source = audio(60_000);
        for text in ["", "   ", "\t"] {
            let draft = draft(vec![segment(0, 1_000, text)]);
            assert!(
                Transcript::from_draft(draft, &source, ProviderId::Deepgram, model(), id(), 1)
                    .is_err(),
                "text {text:?}"
            );
        }
    }

    #[test]
    fn from_draft_rejects_newlines() {
        let source = audio(60_000);
        let draft = draft(vec![segment(0, 1_000, "a\nb")]);
        assert!(
            Transcript::from_draft(draft, &source, ProviderId::Deepgram, model(), id(), 1).is_err()
        );
    }

    #[test]
    fn from_draft_rejects_empty_segment_list() {
        let source = audio(60_000);
        let draft = draft(vec![]);
        assert!(
            Transcript::from_draft(draft, &source, ProviderId::Deepgram, model(), id(), 1).is_err()
        );
    }

    #[test]
    fn from_draft_rejects_too_many_segments() {
        let source = audio(60_000);
        let segments: Vec<Segment> = (0..SEGMENTS_COUNT_MAX as u64 + 1)
            .map(|i| segment(i * 10, i * 10 + 5, "x"))
            .collect();
        let draft = draft(segments);
        assert!(
            Transcript::from_draft(draft, &source, ProviderId::Deepgram, model(), id(), 1).is_err()
        );
    }

    #[test]
    fn from_draft_rejects_oversize_total_text() {
        let source = audio(60_000);
        let segments: Vec<Segment> = (0..10)
            .map(|i| {
                segment(
                    i * 1000,
                    i * 1000 + 950,
                    &"x".repeat(TRANSCRIPT_TEXT_BYTES_MAX / 10),
                )
            })
            .collect();
        let draft = draft(segments);
        assert!(
            Transcript::from_draft(draft, &source, ProviderId::Deepgram, model(), id(), 1).is_err()
        );
    }

    #[test]
    fn text_joins_with_newlines() {
        let source = audio(60_000);
        let draft = draft(vec![segment(0, 1_000, "a"), segment(1_000, 2_000, "b")]);
        let transcript =
            Transcript::from_draft(draft, &source, ProviderId::Deepgram, model(), id(), 1).unwrap();
        assert_eq!(transcript.text(), "a\nb");
    }

    #[test]
    fn from_parts_accepts_end_at_duration_plus_tolerance() {
        let transcript = Transcript::from_parts(
            id(),
            ProviderId::Deepgram,
            model(),
            None,
            1,
            Milliseconds::try_new(60_000).unwrap(),
            vec![segment(0, 61_000, "hello")],
        );
        assert!(transcript.is_ok());
    }

    #[test]
    fn from_parts_rejects_beyond_tolerance() {
        let result = Transcript::from_parts(
            id(),
            ProviderId::Deepgram,
            model(),
            None,
            1,
            Milliseconds::try_new(60_000).unwrap(),
            vec![segment(0, 61_100, "hello")],
        );
        assert!(matches!(result, Err(PartsError::EndBeyondAudio { .. })));
    }
}
