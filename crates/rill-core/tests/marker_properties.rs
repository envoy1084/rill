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
