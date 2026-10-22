//! What structural merge may do.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// `[merge]` in the server configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MergeConfig {
    /// Resolve text conflicts structurally where weave supports the file.
    pub structural: bool,
    /// File extensions never merged structurally (`".yaml"`, `"py"`).
    pub exclude: Vec<String>,
    /// Files larger than this (any side) keep the line merge.
    pub max_bytes: usize,
    /// Longest one structural merge may take.
    pub timeout_ms: u64,
}

impl Default for MergeConfig {
    fn default() -> Self {
        MergeConfig {
            structural: true,
            exclude: Vec::new(),
            max_bytes: 1 << 20,
            timeout_ms: 2_000,
        }
    }
}

impl MergeConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_millis(self.timeout_ms.max(1))
    }
}
