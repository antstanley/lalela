//! Fixture-driven decode tests (development-guidelines.md §Testing).
//! Integration tests may use unwrap/expect freely.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use lalela_core::ports::TranscribeError;
use lalela_voxtral::decode::decode_response;

#[test]
fn fixture_mini_2602_decodes() {
    let bytes = include_bytes!("fixtures/voxtral_mini_2602.json");
    let draft = decode_response(bytes).unwrap();
    assert_eq!(draft.segments.len(), 1);
    assert_eq!(draft.segments[0].text, "Welcome back to the show.");
    assert_eq!(draft.segments[0].end_ms.as_u64(), 2140);
}

#[test]
fn fixture_3_0_decodes() {
    let bytes = include_bytes!("fixtures/voxtral_3_0.json");
    let draft = decode_response(bytes).unwrap();
    assert_eq!(draft.segments.len(), 1);
    assert_eq!(draft.segments[0].start_ms.as_u64(), 400);
    assert!(draft.language.is_some());
}

#[test]
fn empty_body_is_malformed() {
    assert!(matches!(
        decode_response(b""),
        Err(TranscribeError::MalformedResponse { .. })
    ));
}
