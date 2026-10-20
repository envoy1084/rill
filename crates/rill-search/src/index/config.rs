//! Index tuning knobs.

/// Largest blob the index will tokenize, in bytes.
pub const DEFAULT_MAX_BLOB_SIZE: usize = 1024 * 1024;

/// Default number of blobs read and tokenized concurrently by a root walk.
pub const DEFAULT_CONCURRENCY: usize = 16;

/// Configuration of a [`super::SearchIndex`].
#[derive(Debug, Clone)]
pub struct IndexConfig {
    /// Blobs larger than this are recorded as skipped and never searched.
    pub max_blob_size: usize,
    /// Bound on concurrent blob reads while indexing a root.
    pub concurrency: usize,
}

impl Default for IndexConfig {
    fn default() -> Self {
        IndexConfig {
            max_blob_size: DEFAULT_MAX_BLOB_SIZE,
            concurrency: DEFAULT_CONCURRENCY,
        }
    }
}
