//! The audit log's settings.

use serde::{Deserialize, Serialize};

/// `[audit]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AuditConfig {
    /// Seconds between signed seals of each org's chain (when it grew).
    pub seal_every_secs: u64,
}

impl Default for AuditConfig {
    fn default() -> Self {
        AuditConfig {
            seal_every_secs: 86_400,
        }
    }
}
