//! Shared values and the prost encode/decode step.

use prost::Message;
use rill_types::{ObjectHash, ObjectKind, RepoPathBuf};

pub(super) fn h(s: &str) -> ObjectHash {
    ObjectHash::digest(ObjectKind::ChangeVersion, s.as_bytes())
}

pub(super) fn p(s: &str) -> RepoPathBuf {
    RepoPathBuf::new(s).unwrap()
}

/// Encode to bytes and decode again, so prost's own serialization is part of
/// the round trip.
pub(super) fn wire<M: Message + Default>(m: M) -> M {
    M::decode(m.encode_to_vec().as_slice()).unwrap()
}
