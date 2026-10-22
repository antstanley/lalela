//! Sidecar (`<stem>.transcript.json`) encoding and strict decoding
//! (05-persistence.md §Sidecar format).

use lalela_core::audio::AudioSource;
use lalela_core::id::TranscriptId;
use lalela_core::limits::{SEGMENT_TEXT_BYTES_MAX, SEGMENTS_COUNT_MAX};
use lalela_core::ports::{AudioMetadata, ProviderId, StoreError};
use lalela_core::transcript::{Segment, Transcript};
use lalela_core::units::{LanguageTag, Milliseconds, ModelName};
use serde::{Deserialize, Serialize};

/// The sidecar document shape. `deny_unknown_fields` makes every unknown field
/// an error: Lalela is the only writer, so an unknown field means a newer
/// Lalela or manual tampering.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TranscriptDocument {
    /// Schema version; must be 1.
    pub schema_version: u32,
    /// The transcript id.
    pub id: String,
    /// Creation time as Unix milliseconds.
    pub created_at_unix_ms: u64,
    /// The saved audio file's identity.
    pub audio: AudioMetadataDto,
    /// The provider string form.
    pub provider: String,
    /// The model id.
    pub model: String,
    /// The language tag, when known.
    pub language: Option<String>,
    /// The derived plain text (must equal the segments joined by `\n`).
    pub text: String,
    /// The timed segments.
    pub segments: Vec<SegmentDto>,
}

/// The saved audio identity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioMetadataDto {
    /// The audio file's name in the same folder.
    pub file_name: String,
    /// The audio file's size in bytes.
    pub size_bytes: u64,
    /// The audio file's duration in milliseconds.
    pub duration_ms: u64,
}

/// One timed segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentDto {
    /// Start in milliseconds.
    pub start_ms: u64,
    /// End in milliseconds.
    pub end_ms: u64,
    /// The segment text.
    pub text: String,
}

/// Encode a transcript and audio metadata as a pretty-printed sidecar.
pub fn encode_sidecar(transcript: &Transcript, audio: &AudioSource) -> Result<Vec<u8>, StoreError> {
    let document = TranscriptDocument {
        schema_version: 1,
        id: transcript.id.as_str().to_owned(),
        created_at_unix_ms: transcript.created_at_unix_ms,
        audio: AudioMetadataDto {
            file_name: audio.file_name.clone(),
            size_bytes: audio.size_bytes.as_u64(),
            duration_ms: audio.duration_ms.as_u64(),
        },
        provider: transcript.provider.as_str().to_owned(),
        model: transcript.model.as_str().to_owned(),
        language: transcript
            .language
            .as_ref()
            .map(LanguageTag::as_str)
            .map(str::to_owned),
        text: transcript.text(),
        segments: transcript
            .segments
            .iter()
            .map(|segment| SegmentDto {
                start_ms: segment.start_ms.as_u64(),
                end_ms: segment.end_ms.as_u64(),
                text: segment.text.clone(),
            })
            .collect(),
    };
    serde_json::to_vec_pretty(&document).map_err(|error| StoreError::Io {
        detail: format!("sidecar encode failed: {error}"),
    })
}

/// Strictly decode a sidecar into a transcript and its audio metadata.
pub fn decode_sidecar(bytes: &[u8]) -> Result<(Transcript, AudioMetadata), StoreError> {
    let document: TranscriptDocument =
        serde_json::from_slice(bytes).map_err(|error| StoreError::Corrupt {
            detail: format!("sidecar decode failed: {error}"),
        })?;

    if document.schema_version != 1 {
        return Err(StoreError::UnsupportedVersion);
    }

    let id = TranscriptId::try_new(document.id.clone()).ok_or_else(|| StoreError::Corrupt {
        detail: format!("invalid transcript id: {}", document.id),
    })?;
    let provider =
        ProviderId::from_str_form(&document.provider).ok_or_else(|| StoreError::Corrupt {
            detail: format!("unknown provider: {}", document.provider),
        })?;
    let model = ModelName::try_new(document.model.clone()).ok_or_else(|| StoreError::Corrupt {
        detail: format!("invalid model name: {}", document.model),
    })?;
    let language = match document.language {
        None => None,
        Some(tag) => {
            Some(
                LanguageTag::try_new(tag.clone()).ok_or_else(|| StoreError::Corrupt {
                    detail: format!("invalid language tag: {tag}"),
                })?,
            )
        }
    };

    let audio_duration =
        Milliseconds::try_new(document.audio.duration_ms).ok_or_else(|| StoreError::Corrupt {
            detail: "audio duration out of range".to_owned(),
        })?;
    let audio_size = lalela_core::units::Bytes::new(document.audio.size_bytes);

    let mut segments = Vec::with_capacity(document.segments.len());
    for dto in document.segments {
        let start_ms = Milliseconds::try_new(dto.start_ms).ok_or_else(|| StoreError::Corrupt {
            detail: "segment start out of range".to_owned(),
        })?;
        let end_ms = Milliseconds::try_new(dto.end_ms).ok_or_else(|| StoreError::Corrupt {
            detail: "segment end out of range".to_owned(),
        })?;
        segments.push(Segment {
            start_ms,
            end_ms,
            text: dto.text,
        });
    }

    let transcript = Transcript::from_parts(
        id,
        provider,
        model,
        language,
        document.created_at_unix_ms,
        audio_duration,
        segments,
    )
    .map_err(|error| StoreError::Corrupt {
        detail: format!("transcript invariants violated: {error:?}"),
    })?;

    if transcript.text() != document.text {
        return Err(StoreError::Corrupt {
            detail: "sidecar text does not match segments".to_owned(),
        });
    }

    let metadata = AudioMetadata {
        file_name: document.audio.file_name,
        size_bytes: audio_size,
        duration_ms: audio_duration,
    };
    Ok((transcript, metadata))
}

/// Compile-time bounds on the DTO shapes (they mirror canonical-types.schema.json).
const _: () = assert!(SEGMENTS_COUNT_MAX >= 1);
const _: () = assert!(SEGMENT_TEXT_BYTES_MAX >= 1);
