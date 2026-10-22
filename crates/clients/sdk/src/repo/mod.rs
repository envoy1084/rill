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
mod tree;
mod watch;
mod write;

pub use blobs::PUT_BLOB_CHUNK_SIZE;
pub use client::RepoClient;
pub use tree::DirListing;
pub(crate) use watch::convert_stream as watch_stream;
pub use watch::WatchStart;
