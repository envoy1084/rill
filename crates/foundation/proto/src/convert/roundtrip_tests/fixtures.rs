//! Shared values and the wire encode/decode step.

use buffa::Message;
use rill_types::{ObjectHash, ObjectKind, RepoPathBuf};

pub(super) fn h(s: &str) -> ObjectHash {
    ObjectHash::digest(ObjectKind::ChangeVersion, s.as_bytes())
}

pub(super) fn p(s: &str) -> RepoPathBuf {
    RepoPathBuf::new(s).unwrap()
}

/// Encode to bytes and decode again, so the wire serialization is part of
/// the round trip.
pub(super) fn wire<M: Message>(m: M) -> M {
    M::decode_from_slice(&m.encode_to_vec()).unwrap()
}
