//! Content storage: inline, single blob, or a FastCDC-chunked manifest.
//!
//! Size thresholds come from `rill-types` so every producer makes the same
//! choice for the same bytes and content references dedup across clients.
//! Large content can be written from a stream and read back by range, so
//! neither direction holds a whole file in memory.

mod chunking;
mod read;
mod reader;
mod store;
mod stream;
#[cfg(test)]
mod testing;

pub use chunking::{CHUNK_AVG, CHUNK_MAX, CHUNK_MIN};
pub use read::{content_parts, read_content, read_content_range};
pub use reader::BlobReader;
pub use store::{chunk_boundaries, store_content};
pub use stream::{store_content_stream, StoredContent};
