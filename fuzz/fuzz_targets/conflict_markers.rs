//! A working-copy file with arbitrary marker-like content: parsing never
//! panics, and whatever parses re-materializes stably.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_core::{materialize, parse_materialized};

fuzz_target!(|data: &[u8]| {
    if let Some(terms) = parse_materialized(data) {
        let written = materialize(&terms);
        let reparsed = parse_materialized(&written).expect("materialized output parses");
        assert_eq!(materialize(&reparsed), written);
    }
});
