//! A signed seal over an org's chain up to an entry, with the export of
//! the entries it covers.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::hash::ObjectHash;
use crate::time::Timestamp;

/// Seals `first_seq..=last_seq` of `org`'s chain. `head` is the hash of
/// entry `last_seq`; `export` is the blob in the org's object store holding
/// those entries as JSON lines. The signature, by the audit key derived
/// from token key `key_id`, covers [`AuditRoot::signed_bytes`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditRoot {
    pub org: String,
    pub first_seq: u64,
    pub last_seq: u64,
    pub head: [u8; 32],
    pub export: ObjectHash,
    pub sealed_at: Timestamp,
    pub key_id: u32,
    pub public_key: Vec<u8>,
    pub signature: Vec<u8>,
}

impl Record for AuditRoot {
    const TAG: &'static str = "audit-root";
}

impl AuditRoot {
    /// What the signature covers: everything but the key and signature.
    pub fn signed_bytes(&self) -> Vec<u8> {
        let body = (
            &self.org,
            self.first_seq,
            self.last_seq,
            self.head,
            self.export,
            self.sealed_at,
            self.key_id,
        );
        let mut out = b"rill:audit-root:".to_vec();
        out.extend(
            postcard::to_allocvec(&body)
                .expect("postcard serialization of in-memory value cannot fail"),
        );
        out
    }
}
