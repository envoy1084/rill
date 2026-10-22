//! One audited action and its place in the chain.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::principal::Principal;
use crate::time::Timestamp;

/// The stream instance-wide actions (signing keys, revocations) are
/// audited in. Org names cannot start with `_`, so it names no org.
pub const INSTANCE_AUDIT: &str = "_instance";

/// Whether the action happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuditOutcome {
    Allowed,
    Denied { reason: String },
}

/// What happened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRecord {
    /// The org's stream, or [`INSTANCE_AUDIT`].
    pub org: String,
    pub at: Timestamp,
    pub actor: Principal,
    /// Dotted action name, e.g. `relation.add`, `token.issue`.
    pub action: String,
    /// What it acted on, e.g. `repo:acme/web`, `user:ann`.
    pub target: String,
    pub outcome: AuditOutcome,
    pub detail: BTreeMap<String, String>,
}

impl Record for AuditRecord {
    const TAG: &'static str = "audit-record";
}

/// A record placed in its org's chain: `prev` is the hash of the entry
/// before it (zeros for the first), so changing or removing any entry
/// breaks every hash after it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub seq: u64,
    pub prev: [u8; 32],
    pub record: AuditRecord,
}

impl Record for AuditEntry {
    const TAG: &'static str = "audit-entry";
}

impl AuditEntry {
    /// The entry's hash, which the next entry carries as `prev`.
    pub fn hash(&self) -> [u8; 32] {
        let bytes = postcard::to_allocvec(self)
            .expect("postcard serialization of in-memory value cannot fail");
        blake3::derive_key("rill 2026 audit entry", &bytes)
    }

    /// The entry after `head` holding `record`.
    pub fn after(head: &AuditHead, record: AuditRecord) -> Self {
        AuditEntry {
            seq: head.seq + 1,
            prev: head.hash,
            record,
        }
    }
}

/// The end of an org's chain: the last entry's number and hash.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditHead {
    pub seq: u64,
    pub hash: [u8; 32],
}

impl Record for AuditHead {
    const TAG: &'static str = "audit-head";
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(action: &str) -> AuditRecord {
        AuditRecord {
            org: "acme".into(),
            at: Timestamp::from_millis(1),
            actor: Principal::human("ann"),
            action: action.into(),
            target: "repo:web".into(),
            outcome: AuditOutcome::Allowed,
            detail: BTreeMap::new(),
        }
    }

    #[test]
    fn each_entry_seals_everything_before_it() {
        let first = AuditEntry::after(&AuditHead::default(), record("a"));
        let head = AuditHead {
            seq: 1,
            hash: first.hash(),
        };
        let second = AuditEntry::after(&head, record("b"));
        assert_eq!((second.seq, second.prev), (2, first.hash()));
        let mut edited = first.clone();
        edited.record.action = "x".into();
        assert_ne!(edited.hash(), second.prev, "an edit breaks the link");
    }
}
