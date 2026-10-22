//! The remote execution service.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// `[exec]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExecConfig {
    /// Serve REAPI (Execution, ActionCache, CAS, ByteStream, Capabilities)
    /// and the worker protocol.
    pub enabled: bool,
    /// A file of worker secrets, one per line. Workers present one as
    /// `Authorization: Bearer rwk_<secret>`; several allow rotation.
    pub worker_secret_file: Option<PathBuf>,
    /// Seconds an idle worker's request waits for work.
    pub idle_wait_secs: u64,
    /// Seconds between an executing worker's reports.
    pub report_every_secs: u64,
    /// Seconds a worker's claim lasts without a report.
    pub lease_secs: u64,
    /// Workers lost before an action fails.
    pub max_attempts: u32,
    /// Seconds an action may run when it sets no timeout.
    pub default_timeout_secs: u64,
    /// The longest timeout, in seconds, an action may ask for.
    pub max_timeout_secs: u64,
}

impl Default for ExecConfig {
    fn default() -> Self {
        ExecConfig {
            enabled: true,
            worker_secret_file: None,
            idle_wait_secs: 5,
            report_every_secs: 10,
            lease_secs: 45,
            max_attempts: 3,
            default_timeout_secs: 30 * 60,
            max_timeout_secs: 2 * 60 * 60,
        }
    }
}
