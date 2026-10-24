//! The index: segments per shard (blobs by content hash), path layers per
//! root, and how both are written, merged and read.

mod cache;
mod compact;
mod config;
mod records;
mod search_index;
mod views;
mod writer;

pub use compact::CompactStats;
pub use config::IndexConfig;
pub use records::{SegmentRef, SegmentSet, Through, KIND_MARKER, KIND_PATHS, KIND_SEGMENTS};
pub use search_index::{SearchIndex, Snapshot};
pub use writer::IndexStats;
