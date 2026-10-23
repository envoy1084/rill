//! Conflict markers: what is written into a working copy parses back to
//! an equivalent merge.

use bytes::Bytes;
use proptest::prelude::*;
use rill_core::{materialize, parse_materialized};
use rill_types::testing::{merge_of, text};

proptest! {
    #[test]
    fn materialized_conflicts_parse_back(
        terms in merge_of(prop::option::of(text().prop_map(Bytes::from)), 3)
    ) {
        let written = materialize(&terms);
        let parsed = parse_materialized(&written).expect("markers written by materialize parse");
        prop_assert_eq!(materialize(&parsed), written);
    }
}

#[test]
fn marker_like_lines_without_a_conflict_header_are_content() {
    // Found by fuzzing: a plain `<<<<<<<` line made the file unparseable
    // after a round trip.
    let input = b"<<<<<<<\n<<<<<<<<<<<<<<\n++++++++++++++\n>>>>>>>>>>>>>>";
    let parsed = parse_materialized(input).unwrap();
    assert_eq!(parsed.as_resolved(), Some(&Some(Bytes::from_static(input))));
    assert_eq!(materialize(&parsed), input);
}
