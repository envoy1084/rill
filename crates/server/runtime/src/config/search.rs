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
    /// The shards this server serves; every shard when empty. A search
    /// sends each other shard to a live server that serves it.
    pub serve_shards: Vec<u16>,
    /// The URL other servers reach this one's API at, for shard searches;
    /// the bound address when unset (unless it is a wildcard address).
    pub advertise_url: Option<String>,
    /// A CA bundle (PEM) trusted for `https://` peers, besides the system's.
    pub peer_ca_file: Option<PathBuf>,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig {
            shards: 16,
            cache_dir: None,
            segment_mb: 64,
            merge_fanout: 8,
            time_budget_ms: 10_000,
            serve_shards: Vec::new(),
            advertise_url: None,
            peer_ca_file: None,
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
            serve: (!self.serve_shards.is_empty()).then(|| self.serve_shards.clone()),
            ..rill_search::IndexConfig::default()
        }
    }
}
