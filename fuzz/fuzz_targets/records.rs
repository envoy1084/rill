//! Untrusted record bytes: decoding never panics, and a decoded record
//! re-encodes to bytes that decode to the same value.

#![no_main]

use libfuzzer_sys::fuzz_target;
use rill_types::{decode_record, encode_record, ChangeRecord, Intent, Workspace};

fn check<T: rill_types::Record + PartialEq + std::fmt::Debug>(data: &[u8]) {
    if let Ok(value) = decode_record::<T>(data) {
        assert_eq!(decode_record::<T>(&encode_record(&value)).unwrap(), value);
    }
}

fuzz_target!(|data: &[u8]| {
    check::<ChangeRecord>(data);
    check::<Workspace>(data);
    check::<Intent>(data);
});
