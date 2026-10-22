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
    /// Org secrets the command gets, by name.
    pub secrets: Vec<String>,
    /// The sealed command carrying their values while a worker runs it.
    pub secret_command: Option<ObjectHash>,
    /// What the action belongs to, for hermeticity flags:
    /// `verify:<repo>/<step>` or a build tool's target and mnemonic.
    pub label: Option<String>,
    /// A re-run of a cached result, to check it is hermetic.
    pub probe: bool,
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
