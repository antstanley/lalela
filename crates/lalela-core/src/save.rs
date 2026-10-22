//! Save planning as a pure function (05-persistence.md §Save planning).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::audio::AudioSource;
use crate::limits::{FILE_STEM_BYTES_MAX, SAVE_NAME_ATTEMPTS_MAX};

/// Build the in-place plan (the audio already lives in the output folder).
fn in_place_plan(
    audio: &AudioSource,
    output_folder: &Path,
    stem: FileStem,
    names_in_folder: &BTreeSet<String>,
) -> SavePlan {
    let text_target = format!("{}.txt", stem.as_str());
    let sidecar_target = format!("{}.transcript.json", stem.as_str());
    let replaces_existing =
        names_in_folder.contains(&text_target) || names_in_folder.contains(&sidecar_target);
    SavePlan {
        folder: output_folder.to_path_buf(),
        stem,
        audio_target: audio.file_name.clone(),
        text_target,
        sidecar_target,
        copy_audio: false,
        replaces_existing,
    }
}

/// A validated file stem: non-empty, ≤ `FILE_STEM_BYTES_MAX` bytes, no `/`,
/// `:`, NUL, or control characters, and no leading `.`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FileStem(String);

impl FileStem {
    /// Validate and construct a file stem.
    #[must_use]
    pub fn try_new(stem: String) -> Option<Self> {
        if stem.is_empty() || stem.len() > FILE_STEM_BYTES_MAX {
            return None;
        }
        let bytes = stem.as_bytes();
        let ok = bytes[0] != b'.'
            && bytes
                .iter()
                .all(|b| *b != b'/' && *b != b':' && *b != 0 && *b >= 0x20);
        ok.then_some(Self(stem))
    }

    /// The stem value.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Sanitise a source stem into a valid `FileStem`, falling back to
/// `transcript` when nothing survives.
#[must_use]
pub fn sanitize_stem(source: &str) -> String {
    let mut cleaned = String::with_capacity(source.len());
    for ch in source.chars() {
        match ch {
            '/' | ':' | '\0' => cleaned.push('-'),
            c if (c as u32) < 0x20 => cleaned.push('-'),
            c => cleaned.push(c),
        }
    }
    let cleaned = cleaned.trim_start_matches('.');
    let cleaned = cleaned.trim();
    let mut truncated = String::new();
    let mut bytes = 0usize;
    for ch in cleaned.chars() {
        let ch_bytes = ch.len_utf8();
        if bytes + ch_bytes > FILE_STEM_BYTES_MAX {
            break;
        }
        truncated.push(ch);
        bytes += ch_bytes;
    }
    if truncated.is_empty() {
        "transcript".to_owned()
    } else {
        truncated
    }
}

/// Where a transcript lives on disk after a save, or where it was found on
/// reopen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavedPair {
    /// The absolute output folder.
    pub folder: PathBuf,
    /// The shared stem.
    pub stem: FileStem,
    /// The audio file name.
    pub audio_file_name: String,
    /// The text file name.
    pub text_file_name: String,
    /// The sidecar file name.
    pub sidecar_file_name: String,
}

/// The pure output of `plan_save`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SavePlan {
    /// The absolute output folder.
    pub folder: PathBuf,
    /// The shared stem.
    pub stem: FileStem,
    /// The audio target file name.
    pub audio_target: String,
    /// The text target file name.
    pub text_target: String,
    /// The sidecar target file name.
    pub sidecar_target: String,
    /// Whether the audio file needs copying into the folder.
    pub copy_audio: bool,
    /// Whether executing this plan replaces existing files.
    pub replaces_existing: bool,
}

impl SavePlan {
    /// Whether a name for this plan already exists in `names_in_folder`.
    #[must_use]
    pub fn collides(&self, names_in_folder: &BTreeSet<String>) -> bool {
        names_in_folder.contains(&self.audio_target)
            || names_in_folder.contains(&self.text_target)
            || names_in_folder.contains(&self.sidecar_target)
    }
}

/// Why a save could not be planned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveError {
    /// Every numeric suffix is taken.
    NoFreeName,
    /// A folder path is relative; the settings store only stores absolute
    /// paths, so this marks a Lalela bug.
    OutputFolderNotAbsolute,
}

/// Plan a save: target names, collisions, whether the audio needs copying, and
/// whether existing files would be replaced. Pure: no filesystem access.
pub fn plan_save(
    audio: &AudioSource,
    source_folder: &Path,
    output_folder: &Path,
    names_in_folder: &BTreeSet<String>,
) -> Result<SavePlan, SaveError> {
    if !source_folder.is_absolute() || !output_folder.is_absolute() {
        return Err(SaveError::OutputFolderNotAbsolute);
    }

    let raw_stem = audio
        .path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("");

    // The in-place case: the source audio already lives in the output folder
    // and its stem needs no sanitising.
    if source_folder == output_folder
        && let Some(stem) = FileStem::try_new(raw_stem.to_owned())
    {
        return Ok(in_place_plan(audio, output_folder, stem, names_in_folder));
    }

    // The copy case.
    let stem = sanitize_stem(raw_stem);
    assert!(
        FileStem::try_new(stem.clone()).is_some(),
        "sanitize_stem must produce a valid stem"
    );
    let stem_obj = FileStem(stem.clone());
    let extension = audio.format.extension();
    let mut audio_target = format!("{stem}.{extension}");
    let mut text_target = format!("{stem}.txt");
    let mut sidecar_target = format!("{stem}.transcript.json");

    if names_in_folder.contains(&audio_target)
        || names_in_folder.contains(&text_target)
        || names_in_folder.contains(&sidecar_target)
    {
        let mut found = None;
        for attempt in 2..=SAVE_NAME_ATTEMPTS_MAX {
            let suffix = format!(" {attempt}");
            let candidate_audio = format!("{stem}{suffix}.{extension}");
            let candidate_text = format!("{stem}{suffix}.txt");
            let candidate_sidecar = format!("{stem}{suffix}.transcript.json");
            if !names_in_folder.contains(&candidate_audio)
                && !names_in_folder.contains(&candidate_text)
                && !names_in_folder.contains(&candidate_sidecar)
            {
                found = Some((candidate_audio, candidate_text, candidate_sidecar));
                break;
            }
        }
        let Some((audio, text, sidecar)) = found else {
            return Err(SaveError::NoFreeName);
        };
        audio_target = audio;
        text_target = text;
        sidecar_target = sidecar;
    }

    Ok(SavePlan {
        folder: output_folder.to_path_buf(),
        stem: stem_obj,
        audio_target,
        text_target,
        sidecar_target,
        copy_audio: true,
        replaces_existing: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::{AudioFormat, AudioSource};
    use crate::units::{Bytes, Milliseconds};

    fn audio_at(path: &str, format: AudioFormat) -> AudioSource {
        AudioSource::try_new(
            PathBuf::from(path),
            format,
            Bytes::new(1000),
            Milliseconds::try_new(60_000).unwrap(),
        )
        .unwrap()
    }

    fn names() -> BTreeSet<String> {
        BTreeSet::new()
    }

    #[test]
    fn in_place_keeps_name_and_flags_replace() {
        let audio = audio_at("/Users/me/Audio/interview.m4a", AudioFormat::M4a);
        let folder = Path::new("/Users/me/Audio");
        let mut existing = names();
        existing.insert("interview.txt".to_owned());
        let plan = plan_save(&audio, folder, folder, &existing).unwrap();
        assert!(!plan.copy_audio);
        assert!(plan.replaces_existing);
        assert_eq!(plan.audio_target, "interview.m4a");
        assert_eq!(plan.text_target, "interview.txt");
        assert_eq!(plan.sidecar_target, "interview.transcript.json");
        assert_eq!(plan.stem.as_str(), "interview");
    }

    #[test]
    fn copy_case_shared_suffix_on_collision() {
        let audio = audio_at("/tmp/source/interview.m4a", AudioFormat::M4a);
        let source = Path::new("/tmp/source");
        let output = Path::new("/Users/me/Transcripts");
        let mut existing = names();
        existing.insert("interview.m4a".to_owned());
        existing.insert("interview.txt".to_owned());
        existing.insert("interview.transcript.json".to_owned());
        let plan = plan_save(&audio, source, output, &existing).unwrap();
        assert!(plan.copy_audio);
        assert!(!plan.replaces_existing);
        assert_eq!(plan.audio_target, "interview 2.m4a");
        assert_eq!(plan.text_target, "interview 2.txt");
        assert_eq!(plan.sidecar_target, "interview 2.transcript.json");
    }

    #[test]
    fn copy_case_no_collision_keeps_name() {
        let audio = audio_at("/tmp/source/interview.m4a", AudioFormat::M4a);
        let plan = plan_save(
            &audio,
            Path::new("/tmp/source"),
            Path::new("/Users/me/Transcripts"),
            &names(),
        )
        .unwrap();
        assert_eq!(plan.audio_target, "interview.m4a");
        assert_eq!(plan.text_target, "interview.txt");
    }

    #[test]
    fn sanitize_replaces_forbidden() {
        let audio = audio_at("/tmp/src/a:b.m4a", AudioFormat::M4a);
        let plan = plan_save(
            &audio,
            Path::new("/tmp/src"),
            Path::new("/Users/me/Transcripts"),
            &names(),
        )
        .unwrap();
        assert_eq!(plan.stem.as_str(), "a-b");
    }

    #[test]
    fn sanitize_strips_leading_dots() {
        assert_eq!(sanitize_stem(".hidden"), "hidden");
        assert_eq!(sanitize_stem("..dots"), "dots");
        assert_eq!(sanitize_stem("........"), "transcript");
        assert_eq!(sanitize_stem("  "), "transcript");
    }

    #[test]
    fn sanitize_truncates_on_char_boundary() {
        let long = "é".repeat(FILE_STEM_BYTES_MAX + 10);
        let sanitized = sanitize_stem(&long);
        assert!(sanitized.len() <= FILE_STEM_BYTES_MAX);
        assert!(FileStem::try_new(sanitized.clone()).is_some());
        assert!(sanitized.ends_with('é'));
    }

    #[test]
    fn no_free_name_after_attempts() {
        let audio = audio_at("/tmp/src/interview.m4a", AudioFormat::M4a);
        let mut existing = names();
        for attempt in 1..=SAVE_NAME_ATTEMPTS_MAX {
            let suffix = if attempt == 1 {
                String::new()
            } else {
                format!(" {attempt}")
            };
            existing.insert(format!("interview{suffix}.m4a"));
            existing.insert(format!("interview{suffix}.txt"));
            existing.insert(format!("interview{suffix}.transcript.json"));
        }
        let result = plan_save(
            &audio,
            Path::new("/tmp/src"),
            Path::new("/Users/me/Transcripts"),
            &existing,
        );
        assert_eq!(result, Err(SaveError::NoFreeName));
    }

    #[test]
    fn relative_folder_is_error() {
        let audio = audio_at("/tmp/src/interview.m4a", AudioFormat::M4a);
        assert_eq!(
            plan_save(
                &audio,
                Path::new("tmp/src"),
                Path::new("/Users/me/Transcripts"),
                &names()
            ),
            Err(SaveError::OutputFolderNotAbsolute)
        );
        assert_eq!(
            plan_save(
                &audio,
                Path::new("/tmp/src"),
                Path::new("Users/me/Transcripts"),
                &names()
            ),
            Err(SaveError::OutputFolderNotAbsolute)
        );
    }
}
