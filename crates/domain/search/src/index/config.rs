//! Index tuning knobs.

use std::path::PathBuf;

/// Largest blob the index tokenizes, in bytes; larger ones are path-only.
pub const DEFAULT_MAX_BLOB_SIZE: usize = 1024 * 1024;

/// Configuration of a [`super::SearchIndex`].
#[derive(Debug, Clone)]
pub struct IndexConfig {
    /// Blobs larger than this are recorded as skipped and never searched.
    pub max_blob_size: usize,
    /// Bound on concurrent blob reads while indexing.
    pub concurrency: usize,
    /// Shards blobs are spread over by content hash.
    pub shards: u16,
    /// Raw bytes of content after which a new segment is started.
    pub segment_bytes: usize,
    /// Segments of one size tier merged into the next.
    pub merge_fanout: usize,
    /// Where segments are kept on local disk and memory-mapped; in memory
    /// when unset.
    pub cache_dir: Option<PathBuf>,
}

impl Default for IndexConfig {
    fn default() -> Self {
        IndexConfig {
            max_blob_size: DEFAULT_MAX_BLOB_SIZE,
            concurrency: 16,
            shards: 16,
            segment_bytes: 64 * 1024 * 1024,
            merge_fanout: 8,
            cache_dir: None,
        }
    }
}
