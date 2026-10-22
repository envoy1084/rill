//! The remote execution service's process state: the scheduler and the
//! secrets workers present.

use std::path::Path;
use std::time::Duration;

use rill_exec::remote::SchedulerConfig;

use crate::config::ExecConfig;
use crate::error::{Error, Result};

/// The prefix of a worker's bearer token.
pub const WORKER_TOKEN_PREFIX: &str = "rwk_";

/// The scheduler timings `config` asks for.
pub fn scheduler_config(config: &ExecConfig) -> SchedulerConfig {
    SchedulerConfig {
        idle_wait: Duration::from_secs(config.idle_wait_secs),
        report_every: Duration::from_secs(config.report_every_secs),
        lease: Duration::from_secs(config.lease_secs),
        max_attempts: config.max_attempts,
        default_timeout: Duration::from_secs(config.default_timeout_secs),
        max_timeout: Duration::from_secs(config.max_timeout_secs),
        probe_rate: f64::from(config.probe_per_mille.min(1000)) / 1000.0,
    }
}

/// The worker secrets this server accepts, kept as hashes.
#[derive(Default)]
pub struct WorkerSecrets {
    hashes: Vec<blake3::Hash>,
}

impl WorkerSecrets {
    /// Read the secrets in `path`, one per non-empty line.
    pub fn load(path: Option<&Path>) -> Result<Self> {
        let Some(path) = path else {
            return Ok(WorkerSecrets::default());
        };
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("worker secrets {}: {e}", path.display())))?;
        let hashes: Vec<_> = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(|l| blake3::hash(l.as_bytes()))
            .collect();
        if hashes.is_empty() {
            return Err(Error::Config(format!(
                "worker secrets {}: the file has no secret",
                path.display()
            )));
        }
        Ok(WorkerSecrets { hashes })
    }

    /// Whether `token` (a bearer token) is a worker's; comparisons run in
    /// constant time.
    pub fn admits(&self, token: &str) -> bool {
        let Some(secret) = token.strip_prefix(WORKER_TOKEN_PREFIX) else {
            return false;
        };
        let presented = blake3::hash(secret.as_bytes());
        self.hashes
            .iter()
            .fold(false, |found, h| found | (*h == presented))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_listed_secrets_with_the_prefix_are_workers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets");
        std::fs::write(&path, "old-secret\n\nnew-secret\n").unwrap();
        let secrets = WorkerSecrets::load(Some(&path)).unwrap();
        assert!(secrets.admits("rwk_old-secret"));
        assert!(secrets.admits("rwk_new-secret"));
        assert!(!secrets.admits("old-secret"), "the prefix marks a worker");
        assert!(!secrets.admits("rwk_guess"));
        assert!(!WorkerSecrets::default().admits("rwk_old-secret"));
        std::fs::write(&path, "\n").unwrap();
        assert!(WorkerSecrets::load(Some(&path)).is_err());
    }
}
