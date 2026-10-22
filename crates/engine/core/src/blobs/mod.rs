//! Content storage: inline, single blob, or a FastCDC-chunked manifest.
//!
//! Size thresholds come from `rill-types` so every producer makes the same
//! choice for the same bytes and content references dedup across clients.
//! Large content can be written from a stream and read back by range, so
//! neither direction holds a whole file in memory.

mod read;
mod store;
mod stream;

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_store::ObjectStore;
use rill_types::ContentRef;

pub use read::{content_parts, read_content, read_content_range};
pub use store::{chunk_boundaries, store_content};
pub use stream::{store_content_stream, StoredContent};

use crate::error::Result;

/// FastCDC minimum chunk size.
pub const CHUNK_MIN: usize = 16 * 1024;
/// FastCDC average (target) chunk size.
pub const CHUNK_AVG: usize = 64 * 1024;
/// FastCDC maximum chunk size.
pub const CHUNK_MAX: usize = 256 * 1024;

/// Something that can produce the bytes behind a [`ContentRef`].
#[async_trait]
pub trait BlobReader: Send + Sync {
    /// Read the full content of `content`.
    async fn read(&self, content: &ContentRef) -> Result<Bytes>;
}

#[async_trait]
impl BlobReader for Arc<dyn ObjectStore> {
    async fn read(&self, content: &ContentRef) -> Result<Bytes> {
        read_content(self.as_ref(), content).await
    }
}

#[async_trait]
impl<T: BlobReader + ?Sized> BlobReader for Arc<T> {
    async fn read(&self, content: &ContentRef) -> Result<Bytes> {
        (**self).read(content).await
    }
}

#[cfg(test)]
pub(crate) fn pseudo_random(len: usize, seed: u64) -> Bytes {
    let mut state = 0x9E37_79B9_7F4A_7C15u64 ^ seed;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push(state as u8);
    }
    Bytes::from(out)
}
