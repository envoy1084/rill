//! Operations: one request to run an action, from queueing to its result.

use bytes::Bytes;
use rill_types::{ObjectHash, Record, Timestamp};
use serde::{Deserialize, Serialize};

use super::digest::BlobDigest;

/// Where an operation is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Stage {
    Queued,
    Executing,
    Completed,
}

/// A stored operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationRecord {
    pub id: String,
    pub org: String,
    pub action: ObjectHash,
    pub action_size: u64,
    pub stage: Stage,
    /// Who asked first; merged requests share the operation.
    pub requester: String,
    /// The result must not enter the action cache.
    pub do_not_cache: bool,
    pub queued_at: Timestamp,
    pub worker: Option<String>,
    /// The encoded `ExecuteResponse`, once completed.
    pub response: Option<Bytes>,
}

impl Record for OperationRecord {
    const TAG: &'static str = "exec-op";
}

impl OperationRecord {
    pub fn action_digest(&self) -> BlobDigest {
        BlobDigest {
            hash: self.action,
            size: self.action_size,
        }
    }

    /// The operation's name on the wire: `<org>/operations/<id>`.
    pub fn name(&self) -> String {
        operation_name(&self.org, &self.id)
    }
}

pub fn operation_name(org: &str, id: &str) -> String {
    format!("{org}/operations/{id}")
}

/// The org and id in an operation name.
pub fn parse_operation_name(name: &str) -> Option<(&str, &str)> {
    let (org, id) = name.split_once("/operations/")?;
    (!id.is_empty() && !id.contains('/')).then_some((org, id))
}
