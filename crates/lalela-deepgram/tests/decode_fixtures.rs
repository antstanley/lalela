//! Fixture-driven decode tests (development-guidelines.md §Testing).
//! Integration tests may use unwrap/expect freely.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use lalela_core::ports::TranscribeError;
use lalela_deepgram::decode::decode_response;

#[test]
fn fixture_nova3_paragraphs_decodes() {
    let bytes = include_bytes!("fixtures/nova3_paragraphs.json");
    let draft = decode_response(bytes, None).unwrap();
    assert_eq!(draft.segments.len(), 1);
    assert_eq!(draft.segments[0].start_ms.as_u64(), 400);
    assert_eq!(draft.segments[0].end_ms.as_u64(), 4620);
    assert_eq!(draft.language.unwrap().as_str(), "en");
}

#[test]
fn empty_body_is_malformed() {
    assert!(matches!(
        decode_response(b"", None),
        Err(TranscribeError::MalformedResponse { .. })
    ));
}
