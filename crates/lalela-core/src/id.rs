//! Persistent entity IDs (01-domain-model.md §ID scheme).

use std::fmt;

/// A `Transcript` ID: `trn_` followed by a lowercase, hyphenated version 7
/// UUID (RFC 9562).
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TranscriptId(String);

impl TranscriptId {
    /// The `trn_` prefix.
    pub const PREFIX: &'static str = "trn_";

    /// Validate and construct a transcript ID.
    ///
    /// The accepted shape is `trn_<uuid7>` where `<uuid7>` is a lowercase,
    /// hyphenated UUID variant 7: the 13th hex digit must be `7`, and the
    /// 17th must be in `[89ab]`.
    #[must_use]
    pub fn try_new(id: String) -> Option<Self> {
        let body = id.strip_prefix(Self::PREFIX)?;
        if !Self::uuidv7_shape_ok(body) {
            return None;
        }
        Some(Self(id))
    }

    /// The full ID as a string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The 32 lowercase hex digits of the UUID, without hyphens. This seeds
    /// the Voxtral multipart boundary.
    #[must_use]
    pub fn uuid_hex(&self) -> String {
        self.0[Self::PREFIX.len()..]
            .chars()
            .filter(|c| *c != '-')
            .collect()
    }

    /// Whether `body` is a lowercase, hyphenated UUID variant 7.
    fn uuidv7_shape_ok(body: &str) -> bool {
        let bytes = body.as_bytes();
        if bytes.len() != 36 || body.bytes().filter(|b| *b == b'-').count() != 4 {
            return false;
        }
        let valid_hex = |b: u8| b.is_ascii_hexdigit() && !b.is_ascii_uppercase();
        let ranges: [(usize, usize); 5] = [(0, 8), (9, 13), (14, 18), (19, 23), (24, 36)];
        let mut index = 0;
        for (lo, hi) in ranges {
            if !bytes[lo..hi].iter().copied().all(valid_hex) {
                return false;
            }
            index += 1;
        }
        // Variant and version nibbles: version 7 at offset 14, variant [89ab]
        // at offset 19 (after the "01a15dcb-" and "cdc0-" groups).
        let version = bytes[14];
        let variant = bytes[19];
        version == b'7' && (b'8'..=b'b').contains(&variant) && index == 5
    }
}

impl fmt::Debug for TranscriptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Display for TranscriptId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_uuid7() {
        let id = "trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f";
        let parsed = TranscriptId::try_new(id.to_owned());
        assert!(parsed.is_some());
        assert_eq!(parsed.unwrap().as_str(), id);
    }

    #[test]
    fn rejects_bad_prefixes() {
        for bad in [
            "",
            "trn_",
            "trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1",
            "trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1ff",
        ] {
            assert!(TranscriptId::try_new(bad.to_owned()).is_none(), "id {bad}");
        }
    }

    #[test]
    fn rejects_bad_version_and_variant() {
        // Version nibble must be 7, variant must be 8..b.
        let bad_version = "trn_01a15dcb-cdc0-6c4e-9f2d-3a6b5c4d2e1f";
        assert!(TranscriptId::try_new(bad_version.to_owned()).is_none());
        let bad_variant = "trn_01a15dcb-cdc0-7c4e-7f2d-3a6b5c4d2e1f";
        assert!(TranscriptId::try_new(bad_variant.to_owned()).is_none());
    }

    #[test]
    fn rejects_uppercase_and_wrong_dashes() {
        assert!(
            TranscriptId::try_new("trn_01A15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f".to_owned()).is_none()
        );
        assert!(TranscriptId::try_new("trn_01a15dcbcdc07c4e9f2d3a6b5c4d2e1f".to_owned()).is_none());
    }

    #[test]
    fn uuid_hex_strips_hyphens() {
        let id =
            TranscriptId::try_new("trn_01a15dcb-cdc0-7c4e-9f2d-3a6b5c4d2e1f".to_owned()).unwrap();
        let hex = id.uuid_hex();
        assert_eq!(hex.len(), 32);
        assert!(!hex.contains('-'));
        assert!(hex.bytes().all(|b| b.is_ascii_hexdigit()));
    }
}
