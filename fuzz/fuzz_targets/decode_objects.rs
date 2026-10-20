//! Untrusted object bytes: decoding never panics, and anything that decodes
//! re-encodes to bytes that decode to the same value.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_types::{BlobManifest, ChangeVersion, Encodable, Tree};

fn check<T: Encodable + PartialEq + std::fmt::Debug>(data: &[u8]) {
    if let Ok(value) = T::decode(data) {
        let again = T::decode(&value.encode()).expect("re-encoded value decodes");
        assert_eq!(again, value);
    }
}

fuzz_target!(|data: &[u8]| {
    check::<ChangeVersion>(data);
    check::<Tree>(data);
    check::<BlobManifest>(data);
});
