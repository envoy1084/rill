//! A client-supplied unified diff against arbitrary base content: applying
//! never panics and never invents content longer than base plus patch.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use rill_core::apply_unified;

#[derive(Arbitrary, Debug)]
struct Input<'a> {
    base: &'a [u8],
    patch: &'a [u8],
}

fuzz_target!(|input: Input<'_>| {
    if let Ok(out) = apply_unified(input.base, input.patch) {
        assert!(out.len() <= input.base.len() + input.patch.len());
    }
});
