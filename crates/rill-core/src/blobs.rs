//! Content storage: inline, single blob, or FastCDC-chunked manifest.
//!
//! Size thresholds come from `rill-types` so that every producer makes the
//! same choice for the same bytes and content references dedup across
//! clients.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use fastcdc::v2020::FastCDC;
use rill_store::{ObjectStore, ObjectStoreExt};
use rill_types::objects::blob::{ChunkRef, CHUNK_THRESHOLD};
use rill_types::{BlobManifest, ContentRef, ObjectHash, ObjectKind, INLINE_THRESHOLD};

use crate::error::{Error, Result};

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

/// Store `bytes` and return the reference to use in a change.
///
/// Inline for tiny files, one blob object for ordinary files, and a chunk
/// manifest for large ones. Chunking runs on a blocking thread because it
/// scans every byte.
pub async fn store_content(objects: &dyn ObjectStore, bytes: Bytes) -> Result<ContentRef> {
    if bytes.len() <= INLINE_THRESHOLD {
        return Ok(ContentRef::Inline { bytes });
    }
    if (bytes.len() as u64) < CHUNK_THRESHOLD {
        let size = bytes.len() as u64;
        let hash = objects.put_blob(bytes).await?;
        return Ok(ContentRef::Blob { hash, size });
    }
    let size = bytes.len() as u64;
    let data = bytes.clone();
    let cuts = tokio::task::spawn_blocking(move || chunk_boundaries(&data)).await?;
    let mut chunks = Vec::with_capacity(cuts.len());
    for (offset, len) in cuts {
        let chunk = bytes.slice(offset..offset + len);
        let hash = ObjectHash::digest(ObjectKind::Chunk, &chunk);
        objects.put(ObjectKind::Chunk, &hash, chunk).await?;
        chunks.push(ChunkRef {
            hash,
            size: len as u32,
        });
    }
    let manifest = BlobManifest {
        total_size: size,
        chunks,
    };
    let hash = objects.put_object(&manifest).await?;
    Ok(ContentRef::Chunked { hash, size })
}

/// `(offset, length)` of every FastCDC chunk of `data`.
pub fn chunk_boundaries(data: &[u8]) -> Vec<(usize, usize)> {
    FastCDC::new(data, CHUNK_MIN, CHUNK_AVG, CHUNK_MAX)
        .map(|c| (c.offset, c.length))
        .collect()
}

/// Read the bytes behind `content`, reassembling chunks and verifying the
/// total size.
pub async fn read_content(objects: &dyn ObjectStore, content: &ContentRef) -> Result<Bytes> {
    match content {
        ContentRef::Inline { bytes } => Ok(bytes.clone()),
        ContentRef::Blob { hash, size } => {
            let bytes = objects.get_required(ObjectKind::Blob, hash).await?;
            check_size(*size, bytes.len())?;
            Ok(bytes)
        }
        ContentRef::Chunked { hash, size } => {
            let manifest: BlobManifest = objects.get_object_required(hash).await?;
            let keys: Vec<_> = manifest
                .chunks
                .iter()
                .map(|c| (ObjectKind::Chunk, c.hash))
                .collect();
            let parts = objects.get_many(&keys).await?;
            let mut out = BytesMut::with_capacity(*size as usize);
            for (chunk, part) in manifest.chunks.iter().zip(parts) {
                let part = part.ok_or(rill_store::Error::ObjectNotFound {
                    kind: ObjectKind::Chunk,
                    hash: chunk.hash,
                })?;
                if part.len() != chunk.size as usize {
                    return Err(Error::ContentSize {
                        expected: chunk.size as u64,
                        actual: part.len() as u64,
                    });
                }
                out.extend_from_slice(&part);
            }
            check_size(*size, out.len())?;
            Ok(out.freeze())
        }
    }
}

fn check_size(expected: u64, actual: usize) -> Result<()> {
    if expected != actual as u64 {
        return Err(Error::ContentSize {
            expected,
            actual: actual as u64,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_store::MemoryObjectStore;

    fn pseudo_random(len: usize) -> Bytes {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut out = Vec::with_capacity(len);
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            out.push(state as u8);
        }
        Bytes::from(out)
    }

    #[tokio::test]
    async fn picks_representation_by_size() {
        let store = MemoryObjectStore::new();
        let small = store_content(&store, Bytes::from_static(b"hi"))
            .await
            .unwrap();
        assert!(small.is_inline());

        let medium = store_content(&store, pseudo_random(4096)).await.unwrap();
        assert!(matches!(medium, ContentRef::Blob { size: 4096, .. }));

        let big_data = pseudo_random(CHUNK_THRESHOLD as usize + 12345);
        let big = store_content(&store, big_data.clone()).await.unwrap();
        assert!(matches!(big, ContentRef::Chunked { .. }));
        assert_eq!(read_content(&store, &big).await.unwrap(), big_data);
        assert_eq!(read_content(&store, &medium).await.unwrap().len(), 4096);
    }

    #[test]
    fn chunks_cover_input() {
        let data = pseudo_random(2 * 1024 * 1024);
        let cuts = chunk_boundaries(&data);
        assert!(cuts.len() > 1);
        let mut pos = 0;
        for (off, len) in &cuts {
            assert_eq!(*off, pos);
            assert!(*len <= CHUNK_MAX);
            pos += len;
        }
        assert_eq!(pos, data.len());
    }
}
