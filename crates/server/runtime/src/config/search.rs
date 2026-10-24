//! Search index settings.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// `[search]`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchConfig {
    /// Shards blobs are spread over (fixed for a repository's lifetime).
    pub shards: u16,
    /// Local directory segments are cached in and memory-mapped from (a
    /// subdirectory per repository); in memory when unset.
    pub cache_dir: Option<PathBuf>,
    /// Megabytes of content per freshly written segment.
    pub segment_mb: usize,
    /// Segments of one tier merged into the next.
    pub merge_fanout: usize,
    /// Milliseconds a query may run before it returns partial results.
    pub time_budget_ms: u64,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig {
            shards: 16,
            cache_dir: None,
            segment_mb: 64,
            merge_fanout: 8,
            time_budget_ms: 10_000,
        }
    }
}

impl SearchConfig {
    /// The index configuration for one repository.
    pub fn index(&self, repo: &rill_types::RepoId) -> rill_search::IndexConfig {
        rill_search::IndexConfig {
            shards: self.shards.max(1),
            cache_dir: self.cache_dir.as_ref().map(|d| d.join(repo.to_string())),
            segment_bytes: self.segment_mb.max(1) * 1024 * 1024,
            merge_fanout: self.merge_fanout.max(2),
            ..rill_search::IndexConfig::default()
        }
    }
}
