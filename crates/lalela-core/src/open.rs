//! Classifying what the user opened (05-persistence.md §Reopening).

use std::path::{Path, PathBuf};

use crate::audio::AudioFormat;

/// What a path resolves to when the user opens it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenTarget {
    /// A sidecar (`*.transcript.json`) itself.
    Sidecar(PathBuf),
    /// An audio file whose sibling sidecar exists.
    AudioWithSidecar { audio: PathBuf, sidecar: PathBuf },
    /// An audio file with no sibling sidecar.
    AudioOnly(PathBuf),
}

/// Why a path could not be classified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenError {
    /// The file is not a sidecar or an allowed audio file.
    UnsupportedFile,
}

/// Classify a path the user opened. Pure: the caller decides whether a sibling
/// sidecar exists and passes the answer in, so the core does no I/O.
pub fn classify_open_path(
    path: &Path,
    sibling_sidecar_exists: bool,
) -> Result<OpenTarget, OpenError> {
    let file_name = path.file_name().and_then(|name| name.to_str());
    let Some(file_name) = file_name else {
        return Err(OpenError::UnsupportedFile);
    };

    if file_name.ends_with(".transcript.json") {
        return Ok(OpenTarget::Sidecar(path.to_path_buf()));
    }

    if AudioFormat::from_path(path).is_some() {
        if sibling_sidecar_exists {
            let sidecar = path.with_extension("transcript.json");
            return Ok(OpenTarget::AudioWithSidecar {
                audio: path.to_path_buf(),
                sidecar,
            });
        }
        return Ok(OpenTarget::AudioOnly(path.to_path_buf()));
    }

    Err(OpenError::UnsupportedFile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_sidecar() {
        let path = Path::new("/Users/me/Transcripts/interview.transcript.json");
        assert_eq!(
            classify_open_path(path, false),
            Ok(OpenTarget::Sidecar(path.to_path_buf()))
        );
    }

    #[test]
    fn classifies_audio_with_sibling() {
        let path = Path::new("/Users/me/Transcripts/interview.m4a");
        assert_eq!(
            classify_open_path(path, true),
            Ok(OpenTarget::AudioWithSidecar {
                audio: path.to_path_buf(),
                sidecar: Path::new("/Users/me/Transcripts/interview.transcript.json").to_path_buf(),
            })
        );
        assert_eq!(
            classify_open_path(path, false),
            Ok(OpenTarget::AudioOnly(path.to_path_buf()))
        );
    }

    #[test]
    fn rejects_unknown_extensions() {
        assert_eq!(
            classify_open_path(Path::new("/tmp/notes.txt"), false),
            Err(OpenError::UnsupportedFile)
        );
        assert_eq!(
            classify_open_path(Path::new("/tmp/clip.ogg"), false),
            Err(OpenError::UnsupportedFile)
        );
        assert_eq!(
            classify_open_path(Path::new("/tmp/noext"), false),
            Err(OpenError::UnsupportedFile)
        );
    }

    #[test]
    fn uppercased_audio_extensions_are_allowed() {
        let path = Path::new("/tmp/clip.M4A");
        assert_eq!(
            classify_open_path(path, false),
            Ok(OpenTarget::AudioOnly(path.to_path_buf()))
        );
    }
}
