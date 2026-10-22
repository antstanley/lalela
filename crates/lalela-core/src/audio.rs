//! Audio file identity and shape (01-domain-model.md §AudioSource).

use std::path::{Path, PathBuf};

use crate::limits::{AUDIO_BYTES_MAX, AUDIO_DURATION_MS_MAX};
use crate::units::{Bytes, Milliseconds};

/// The audio container format, derived from the lowercase file extension.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AudioFormat {
    /// `.mp3`
    Mp3,
    /// `.m4a`
    M4a,
    /// `.mp4`
    Mp4,
    /// `.aac`
    Aac,
    /// `.wav`
    Wav,
    /// `.aiff`
    Aiff,
    /// `.caf`
    Caf,
    /// `.flac`
    Flac,
}

impl AudioFormat {
    /// Map a file extension (without the leading dot, any case) to a format.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        match extension.to_ascii_lowercase().as_str() {
            "mp3" => Some(Self::Mp3),
            "m4a" => Some(Self::M4a),
            "mp4" => Some(Self::Mp4),
            "aac" => Some(Self::Aac),
            "wav" => Some(Self::Wav),
            "aiff" => Some(Self::Aiff),
            "caf" => Some(Self::Caf),
            "flac" => Some(Self::Flac),
            _ => None,
        }
    }

    /// The MIME type for this format.
    #[must_use]
    pub const fn mime(&self) -> &'static str {
        match self {
            Self::Mp3 => "audio/mpeg",
            Self::M4a => "audio/mp4",
            Self::Mp4 => "audio/mp4",
            Self::Aac => "audio/aac",
            Self::Wav => "audio/wav",
            Self::Aiff => "audio/aiff",
            Self::Caf => "audio/x-caf",
            Self::Flac => "audio/flac",
        }
    }

    /// The canonical lowercase extension, without a leading dot.
    #[must_use]
    pub const fn extension(&self) -> &'static str {
        match self {
            Self::Mp3 => "mp3",
            Self::M4a => "m4a",
            Self::Mp4 => "mp4",
            Self::Aac => "aac",
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::Caf => "caf",
            Self::Flac => "flac",
        }
    }

    /// Whether `path` has an allowed audio extension.
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        Self::from_extension(extension)
    }
}

/// Why an `AudioSource` could not be built from a file the player loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AudioSourceError {
    /// The probe reported no duration; the file may be corrupt or empty.
    NoDuration,
    /// The file is longer than `AUDIO_DURATION_MS_MAX`.
    DurationTooLong { duration_ms: u64 },
    /// The file is larger than `AUDIO_BYTES_MAX`.
    TooLarge { size_bytes: u64 },
    /// The file is empty.
    Empty,
}

/// The audio file the session is working on, built after the player loaded the
/// file and reported its duration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioSource {
    /// Absolute path of the file as opened.
    pub path: PathBuf,
    /// The final path component, valid UTF-8.
    pub file_name: String,
    /// The audio format derived from the extension.
    pub format: AudioFormat,
    /// File size in bytes.
    pub size_bytes: Bytes,
    /// Probed duration in milliseconds.
    pub duration_ms: Milliseconds,
}

impl AudioSource {
    /// Validate and construct an audio source.
    ///
    /// The caller has already loaded the file through the player, so the
    /// duration is the player's probe. The size comes from metadata.
    pub fn try_new(
        path: PathBuf,
        format: AudioFormat,
        size_bytes: Bytes,
        duration_ms: Milliseconds,
    ) -> Result<Self, AudioSourceError> {
        if size_bytes.as_u64() == 0 {
            return Err(AudioSourceError::Empty);
        }
        if size_bytes.as_u64() > AUDIO_BYTES_MAX {
            return Err(AudioSourceError::TooLarge {
                size_bytes: size_bytes.as_u64(),
            });
        }
        let duration = duration_ms.as_u64();
        if duration == 0 {
            return Err(AudioSourceError::NoDuration);
        }
        if duration > AUDIO_DURATION_MS_MAX {
            return Err(AudioSourceError::DurationTooLong {
                duration_ms: duration,
            });
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(AudioSourceError::Empty)?;
        let file_name = file_name.to_owned();
        Ok(Self {
            path,
            file_name,
            format,
            size_bytes,
            duration_ms,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_extensions() {
        assert_eq!(AudioFormat::from_extension("m4a"), Some(AudioFormat::M4a));
        assert_eq!(AudioFormat::from_extension("M4A"), Some(AudioFormat::M4a));
        assert_eq!(AudioFormat::from_extension("wav"), Some(AudioFormat::Wav));
        assert_eq!(AudioFormat::from_extension("ogg"), None);
        assert_eq!(AudioFormat::from_extension(""), None);
    }

    #[test]
    fn source_valid_edges() {
        assert!(
            AudioSource::try_new(
                PathBuf::from("/tmp/a.m4a"),
                AudioFormat::M4a,
                Bytes::new(1),
                Milliseconds::try_new(AUDIO_DURATION_MS_MAX).unwrap(),
            )
            .is_ok()
        );
        assert!(
            AudioSource::try_new(
                PathBuf::from("/tmp/a.m4a"),
                AudioFormat::M4a,
                Bytes::new(1),
                Milliseconds::try_new(1).unwrap(),
            )
            .is_ok()
        );
    }

    #[test]
    fn source_rejects_out_of_bounds() {
        assert_eq!(
            AudioSource::try_new(
                PathBuf::from("/tmp/a.m4a"),
                AudioFormat::M4a,
                Bytes::new(0),
                Milliseconds::try_new(1_000).unwrap(),
            ),
            Err(AudioSourceError::Empty)
        );
        assert_eq!(
            AudioSource::try_new(
                PathBuf::from("/tmp/a.m4a"),
                AudioFormat::M4a,
                Bytes::new(1),
                Milliseconds::try_new(0).unwrap(),
            ),
            Err(AudioSourceError::NoDuration)
        );
        assert_eq!(
            AudioSource::try_new(
                PathBuf::from("/tmp/a.m4a"),
                AudioFormat::M4a,
                Bytes::new(1),
                Milliseconds::try_new(AUDIO_DURATION_MS_MAX + 1).unwrap(),
            ),
            Err(AudioSourceError::DurationTooLong {
                duration_ms: AUDIO_DURATION_MS_MAX + 1,
            })
        );
        assert_eq!(
            AudioSource::try_new(
                PathBuf::from("/tmp/a.m4a"),
                AudioFormat::M4a,
                Bytes::new(AUDIO_BYTES_MAX + 1),
                Milliseconds::try_new(1_000).unwrap(),
            ),
            Err(AudioSourceError::TooLarge {
                size_bytes: AUDIO_BYTES_MAX + 1,
            })
        );
    }
}
