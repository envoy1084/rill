//! Redactions: an object whose content is never served again, applied
//! only when a second administrator approves the request.

use serde::{Deserialize, Serialize};

use crate::encoding::Record;
use crate::hash::{ObjectHash, ObjectKind};
use crate::principal::Principal;
use crate::time::Timestamp;

/// A request to redact an object of an org, and its approval.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redaction {
    /// `red_<hex>`.
    pub id: String,
    pub org: String,
    pub kind: ObjectKind,
    pub hash: ObjectHash,
    pub reason: String,
    pub requested_by: Principal,
    pub requested_at: Timestamp,
    /// Set when applied; never the requester.
    pub approved_by: Option<Principal>,
    pub applied_at: Option<Timestamp>,
}

impl Record for Redaction {
    const TAG: &'static str = "redaction";
}

impl Redaction {
    pub fn is_applied(&self) -> bool {
        self.applied_at.is_some()
    }
}
