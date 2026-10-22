//! Tagged newtypes for units and small values (01-domain-model.md §Value types).

use crate::limits::{
    API_KEY_BYTES_MAX, API_KEY_BYTES_MIN, LANGUAGE_TAG_BYTES_MAX, MILLISECONDS_MAX_U64,
};

/// Media time or duration in whole milliseconds, bounded by
/// `MILLISECONDS_MAX_U64`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Milliseconds(u64);

impl Milliseconds {
    /// Try to construct a `Milliseconds` value within the media-time bound.
    #[must_use]
    pub fn try_new(value: u64) -> Option<Self> {
        (value <= MILLISECONDS_MAX_U64).then_some(Self(value))
    }

    /// The raw millisecond value.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }

    /// The difference `self - other` in milliseconds, saturating at zero.
    /// Both operands are valid media times, so the result is a non-negative
    /// number of milliseconds.
    #[must_use]
    pub fn ms_past(self, other: Self) -> u64 {
        self.0.saturating_sub(other.0)
    }
}

/// A size in bytes. Call sites bound the values they accept.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Bytes(u64);

impl Bytes {
    /// Construct a `Bytes` value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The raw byte count.
    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// A BCP-47 language tag: a 2–3 lowercase-letter primary subtag, an optional
/// 4-letter titlecase script subtag, and an optional 2-letter uppercase or
/// 3-digit region subtag (01-domain-model.md §Value types).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LanguageTag(String);

impl LanguageTag {
    /// Validate a language tag shape. Returns `None` when the tag does not fit
    /// the documented BCP-47 shape or the byte cap.
    #[must_use]
    pub fn try_new(tag: String) -> Option<Self> {
        if tag.len() > LANGUAGE_TAG_BYTES_MAX || !Self::shape_ok(&tag) {
            return None;
        }
        Some(Self(tag))
    }

    /// True when `tag` matches `primary (-script)? (-region)?`.
    fn shape_ok(tag: &str) -> bool {
        let mut parts = tag.split('-');
        let Some(primary) = parts.next() else {
            return false;
        };
        if primary.len() < 2
            || primary.len() > 3
            || !primary.bytes().all(|b| b.is_ascii_lowercase())
        {
            return false;
        }
        // Optional script subtag: exactly 4 characters, titlecase.
        if let Some(next) = parts.clone().next()
            && next.len() == 4
            && next.as_bytes()[0].is_ascii_uppercase()
            && next.as_bytes()[1..].iter().all(u8::is_ascii_lowercase)
        {
            parts.next();
        }
        // Optional region subtag: 2 uppercase letters or 3 digits.
        if let Some(region) = parts.next() {
            let region_ok = (region.len() == 2 && region.bytes().all(|b| b.is_ascii_uppercase()))
                || (region.len() == 3 && region.bytes().all(|b| b.is_ascii_digit()));
            if !region_ok {
                return false;
            }
        }
        parts.next().is_none()
    }

    /// The primary subtag (everything before the first `-`).
    #[must_use]
    pub fn primary_subtag(&self) -> &str {
        self.0.split('-').next().unwrap_or(&self.0)
    }

    /// The full tag as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The language selection for a transcription: automatic detection or an
/// explicit tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LanguagePreference {
    /// Let the provider detect the language.
    Auto,
    /// An explicit language tag.
    Tag(LanguageTag),
}

impl LanguagePreference {
    /// The explicit tag, when the preference is `Tag`.
    #[must_use]
    pub fn tag(&self) -> Option<&LanguageTag> {
        match self {
            Self::Auto => None,
            Self::Tag(tag) => Some(tag),
        }
    }
}

/// A provider model identifier as sent to the provider: lowercase
/// `[a-z0-9.-]`, starting with a letter or digit, 1–64 bytes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ModelName(String);

impl ModelName {
    /// Validate a model name.
    #[must_use]
    pub fn try_new(name: String) -> Option<Self> {
        let bytes = name.as_bytes();
        if bytes.is_empty() || bytes.len() > 64 {
            return None;
        }
        let first_ok = bytes[0].is_ascii_alphanumeric();
        let rest_ok = bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || *b == b'.' || *b == b'-');
        (first_ok && rest_ok).then_some(Self(name))
    }

    /// The model id as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An API key: printable ASCII without whitespace, between
/// `API_KEY_BYTES_MIN` and `API_KEY_BYTES_MAX` bytes. `Debug` and `Display`
/// redact the value.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    /// Validate and construct an API key.
    #[must_use]
    pub fn try_new(key: String) -> Option<Self> {
        let bytes = key.as_bytes();
        let length_ok = (API_KEY_BYTES_MIN..=API_KEY_BYTES_MAX).contains(&bytes.len());
        let printable_ok = bytes.iter().all(|b| (0x21..=0x7e).contains(b));
        (length_ok && printable_ok).then_some(Self(key))
    }

    /// The key value for use in request headers. This is the only accessor
    /// that reveals the secret, and it is never logged.
    #[must_use]
    pub fn as_secret(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

impl std::fmt::Display for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::MILLISECONDS_MAX_U64;

    #[test]
    fn milliseconds_bound() {
        assert!(Milliseconds::try_new(0).is_some());
        assert!(Milliseconds::try_new(MILLISECONDS_MAX_U64).is_some());
        assert!(Milliseconds::try_new(MILLISECONDS_MAX_U64 + 1).is_none());
    }

    #[test]
    fn milliseconds_past_saturates() {
        let a = Milliseconds::try_new(1_000).unwrap();
        let b = Milliseconds::try_new(2_000).unwrap();
        assert_eq!(a.ms_past(b), 0);
        assert_eq!(b.ms_past(a), 1_000);
    }

    #[test]
    fn language_tag_accepts_valid_shapes() {
        for tag in [
            "en",
            "en-US",
            "fr",
            "pt-BR",
            "zh-Hant-TW",
            "es-419",
            "de-DE",
        ] {
            assert!(LanguageTag::try_new(tag.to_owned()).is_some(), "tag {tag}");
        }
    }

    #[test]
    fn language_tag_rejects_invalid_shapes() {
        for tag in [
            "", "e", "EN", "en_US", "en-", "en-US-", "en--US", "x", "a-b-c-d",
        ] {
            assert!(LanguageTag::try_new(tag.to_owned()).is_none(), "tag {tag}");
        }
        let too_long = format!("{}-Xxxx-999", "x".repeat(12));
        assert!(LanguageTag::try_new(too_long).is_none());
    }

    #[test]
    fn language_tag_primary_subtag() {
        let tag = LanguageTag::try_new("en-GB".to_owned()).unwrap();
        assert_eq!(tag.primary_subtag(), "en");
        let tag = LanguageTag::try_new("zh-Hant-TW".to_owned()).unwrap();
        assert_eq!(tag.primary_subtag(), "zh");
    }

    #[test]
    fn model_name_rules() {
        assert!(ModelName::try_new("nova-3".to_owned()).is_some());
        assert!(ModelName::try_new("voxtral-mini-transcribe-3-0".to_owned()).is_some());
        assert!(ModelName::try_new("".to_owned()).is_none());
        assert!(ModelName::try_new("-nova".to_owned()).is_none());
        assert!(ModelName::try_new("nova 3".to_owned()).is_none());
        assert!(ModelName::try_new("nova_3".to_owned()).is_none());
        assert!(ModelName::try_new("n".repeat(65)).is_none());
    }

    #[test]
    fn api_key_rules() {
        assert!(ApiKey::try_new("abcdefgh".to_owned()).is_some());
        assert!(ApiKey::try_new("abc".to_owned()).is_none());
        assert!(ApiKey::try_new("abcdefgh ".to_owned()).is_none());
        assert!(ApiKey::try_new("abcdefgh\t".to_owned()).is_none());
        assert!(ApiKey::try_new("abcdefghé".to_owned()).is_none());
        assert!(ApiKey::try_new("a".repeat(513)).is_none());
        assert!(ApiKey::try_new("a".repeat(512)).is_some());
    }

    #[test]
    fn api_key_redacts() {
        let key = ApiKey::try_new("supersecret".to_owned()).unwrap();
        assert!(!format!("{key:?}").contains("supersecret"));
        assert!(!format!("{key}").contains("supersecret"));
        assert_eq!(key.as_secret(), "supersecret");
    }
}
