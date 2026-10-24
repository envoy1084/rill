//! Repository-scoped operations: history, trees, blobs, diffs, writes,
//! refs, watches and derived data.

mod blobs;
mod catalog;
mod client;
mod compare;
mod derived;
mod history;
mod refs;
mod search;
mod symbols;
mod tree;
mod watch;
mod write;

pub use blobs::PUT_BLOB_CHUNK_SIZE;
pub use client::RepoClient;
pub use tree::DirListing;
pub use watch::WatchStart;
pub(crate) use watch::{convert_stream as watch_stream, map_stream};
