//! Finding the segment under the playhead (04-playback.md §Finding the current
//! segment).

use crate::limits::SEGMENT_RESTART_THRESHOLD_MS;
use crate::transcript::Segment;
use crate::units::Milliseconds;

/// Find the index of the segment containing `position`, or `None` when the
/// playhead is in a gap (or before the first segment).
///
/// Segments must be non-empty with non-decreasing `start_ms` values (guaranteed
/// by `Transcript` construction; re-checked in debug builds because the check
/// is O(n)).
#[must_use]
pub fn segment_at_position(segments: &[Segment], position: Milliseconds) -> Option<u32> {
    assert!(!segments.is_empty(), "segments must be non-empty");
    debug_assert!(
        segments
            .windows(2)
            .all(|pair| pair[0].start_ms <= pair[1].start_ms),
        "segments must be ordered by start_ms"
    );
    let count = segments.partition_point(|segment| segment.start_ms <= position);
    if count == 0 {
        return None;
    }
    let index = count - 1;
    let segment = &segments[index];
    assert!(index < segments.len());
    (position < segment.end_ms).then_some(index as u32)
}

/// The segment after `current`.
#[must_use]
pub fn segment_next_index(current: u32, len: usize) -> Option<u32> {
    assert!(len > 0);
    let next = u64::from(current) + 1;
    (next < len as u64).then_some(next as u32)
}

/// The segment to skip to with `⌥←`: restart the current segment when the
/// playhead is more than `SEGMENT_RESTART_THRESHOLD_MS` into it, otherwise go
/// to the previous segment. `None` when there is nowhere to go.
#[must_use]
pub fn segment_previous_index(
    current: u32,
    position: Milliseconds,
    segments: &[Segment],
) -> Option<u32> {
    assert!(!segments.is_empty());
    let index = current as usize;
    assert!(index < segments.len());
    let segment = &segments[index];
    if position.ms_past(segment.start_ms) > SEGMENT_RESTART_THRESHOLD_MS {
        return Some(current);
    }
    if current > 0 { Some(current - 1) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn segment(start: u64, end: u64) -> Segment {
        Segment {
            start_ms: Milliseconds::try_new(start).unwrap(),
            end_ms: Milliseconds::try_new(end).unwrap(),
            text: "hello".to_owned(),
        }
    }

    fn sorted_segments() -> Vec<Segment> {
        vec![
            segment(0, 1_000),
            segment(1_000, 2_500),
            segment(5_000, 6_000),
            segment(8_000, 9_000),
        ]
    }

    #[test]
    fn position_finds_segment() {
        let segments = sorted_segments();
        assert_eq!(
            segment_at_position(&segments, Milliseconds::try_new(0).unwrap()),
            Some(0)
        );
        assert_eq!(
            segment_at_position(&segments, Milliseconds::try_new(999).unwrap()),
            Some(0)
        );
        assert_eq!(
            segment_at_position(&segments, Milliseconds::try_new(1_000).unwrap()),
            Some(1)
        );
        assert_eq!(
            segment_at_position(&segments, Milliseconds::try_new(2_499).unwrap()),
            Some(1)
        );
        assert_eq!(
            segment_at_position(&segments, Milliseconds::try_new(5_500).unwrap()),
            Some(2)
        );
        assert_eq!(
            segment_at_position(&segments, Milliseconds::try_new(8_999).unwrap()),
            Some(3)
        );
    }

    #[test]
    fn position_gaps_return_none() {
        let segments = sorted_segments();
        assert_eq!(
            segment_at_position(&segments, Milliseconds::try_new(3_000).unwrap()),
            None
        );
        assert_eq!(
            segment_at_position(&segments, Milliseconds::try_new(9_001).unwrap()),
            None
        );
        assert_eq!(
            segment_at_position(&segments, Milliseconds::try_new(20_000).unwrap()),
            None
        );
    }

    #[test]
    fn next_and_previous() {
        let segments = sorted_segments();
        assert_eq!(segment_next_index(0, segments.len()), Some(1));
        assert_eq!(segment_next_index(3, segments.len()), None);
        assert_eq!(
            segment_previous_index(1, Milliseconds::try_new(1_000).unwrap(), &segments),
            Some(0)
        );
        // Deep into segment 1 (more than the restart threshold): restart it.
        assert_eq!(
            segment_previous_index(1, Milliseconds::try_new(3_100).unwrap(), &segments),
            Some(1)
        );
        // At the first segment: nowhere to go.
        assert_eq!(
            segment_previous_index(0, Milliseconds::try_new(500).unwrap(), &segments),
            None
        );
    }

    proptest! {
        #[test]
        fn agrees_with_linear_scan(
            starts in prop::collection::vec(0u64..10_000, 1..50),
            position in 0u64..12_000,
        ) {
            let mut starts = starts;
            starts.sort_unstable();
            starts.dedup();
            let segments: Vec<Segment> = starts
                .iter()
                .map(|&start| segment(start, start + 1_000))
                .collect();
            // Reference: linear scan for the *last* segment that contains the
            // position (with overlapping segments the search picks the latest
            // one, matching the binary-search contract).
            let expected = (0..segments.len())
                .rev()
                .find(|&i| {
                    segments[i].start_ms.as_u64() <= position
                        && position < segments[i].end_ms.as_u64()
                });
            let got = segment_at_position(&segments, Milliseconds::try_new(position).unwrap());
            assert_eq!(got.map(u64::from), expected.map(|i| i as u64));
        }
    }
}
