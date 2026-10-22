//! Canned hashes and change metadata for MCP tests.

use rill_proto::convert::{hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, ObjectHash, ObjectKind, Principal};

pub fn fake_hash(seed: &str) -> ObjectHash {
    ObjectHash::digest(ObjectKind::ChangeVersion, seed.as_bytes())
}

pub fn sample_info(seed: &str, seq: Option<u64>) -> pb::ChangeInfo {
    pb::ChangeInfo {
        hash: hash_to_proto(&fake_hash(seed)),
        change_id: id_to_proto(&ChangeId::derived("test", seed)),
        parents: vec![],
        predecessor: None,
        author: Some(pb::Principal::from(Principal::human("alice"))),
        provenance: Some(pb::Provenance::default()),
        intent: None,
        message: format!("change {seed}\n\nbody"),
        created_at: 1_700_000_000_000,
        files_changed: 2,
        has_conflicts: false,
        trunk_seq: seq,
    }
}
