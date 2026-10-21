//! Untrusted protobuf: decoding and converting never panic, and what
//! converts comes back unchanged through the wire form.

#![no_main]

use libfuzzer_sys::fuzz_target;
use prost::Message;
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeVersion, Tree};

fuzz_target!(|data: &[u8]| {
    if let Ok(wire) = pb::ChangeVersion::decode(data) {
        if let Ok(cv) = ChangeVersion::try_from(wire) {
            let back = ChangeVersion::try_from(pb::ChangeVersion::from(&cv)).unwrap();
            assert_eq!(back, cv);
        }
    }
    if let Ok(wire) = pb::Tree::decode(data) {
        let _ = Tree::try_from(wire);
    }
    let _ = pb::ApplyEditsRequest::decode(data);
    let _ = pb::PutBlobRequest::decode(data);
});
