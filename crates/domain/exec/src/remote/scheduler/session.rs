//! What a worker is executing, kept between its reports, which may reach
//! different servers.

use bytes::Bytes;
use rill_jobs::Claim;
use rill_types::{ObjectHash, Record, Timestamp};
use serde::{Deserialize, Serialize};

/// A worker's current action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSession {
    pub org: String,
    pub queue: String,
    pub claim: Claim,
    pub operation: String,
    pub action: ObjectHash,
    pub action_size: u64,
    /// The encoded `Action`, sent again with each report's reply.
    pub action_bytes: Bytes,
    pub queued_at: Timestamp,
}

impl Record for WorkerSession {
    const TAG: &'static str = "exec-worker";
}

/// A stable key for a worker from its identifying labels.
pub fn worker_key(labels: &std::collections::HashMap<String, String>) -> String {
    let mut pairs: Vec<_> = labels.iter().collect();
    pairs.sort_unstable();
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(",")
}
