//! The remote execution service.

use std::collections::BTreeMap;
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
    /// Results entering the action cache run again, per thousand, to check
    /// they are hermetic.
    pub probe_per_mille: u32,
    /// Workers inside this server.
    pub local: LocalWorkersConfig,
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
            probe_per_mille: 20,
            local: LocalWorkersConfig::default(),
        }
    }
}

/// `[exec.local]`: workers that run actions in this server's process,
/// for development and single-host setups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalWorkersConfig {
    /// How many; each runs one action at a time.
    pub workers: usize,
    /// How their commands are isolated.
    pub sandbox: SandboxKind,
    /// Whether commands may use the network (advisory under `local`).
    pub network: bool,
    /// The `runsc` binary for the `gvisor` sandbox.
    pub runsc: String,
    /// The platform properties they serve; actions asking for others wait
    /// for other workers.
    pub platform: BTreeMap<String, String>,
}

impl Default for LocalWorkersConfig {
    fn default() -> Self {
        LocalWorkersConfig {
            workers: 0,
            sandbox: SandboxKind::Local,
            network: false,
            runsc: "runsc".into(),
            platform: BTreeMap::new(),
        }
    }
}

/// How a local worker isolates commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SandboxKind {
    /// On the host in a private directory, unisolated.
    #[default]
    Local,
    /// Linux only: Landlock, seccomp and resource limits.
    Confined,
    /// Each command under gVisor's `runsc do`.
    Gvisor,
}
