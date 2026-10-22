//! Storing content held in memory.

use bytes::Bytes;
use fastcdc::v2020::FastCDC;
use rill_store::{ObjectStore, ObjectStoreExt};
use rill_types::objects::blob::{ChunkRef, CHUNK_THRESHOLD};
use rill_types::{BlobManifest, ContentRef, ObjectHash, ObjectKind, INLINE_THRESHOLD};

use super::{CHUNK_AVG, CHUNK_MAX, CHUNK_MIN};
use crate::error::Result;

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
    let data = bytes.clone();
    let cuts = tokio::task::spawn_blocking(move || chunk_boundaries(&data)).await?;
    let mut chunks = Vec::with_capacity(cuts.len());
    for (offset, len) in cuts {
        chunks.push(put_chunk(objects, bytes.slice(offset..offset + len)).await?);
    }
    put_manifest(objects, chunks).await
}

/// `(offset, length)` of every FastCDC chunk of `data`.
pub fn chunk_boundaries(data: &[u8]) -> Vec<(usize, usize)> {
    FastCDC::new(data, CHUNK_MIN, CHUNK_AVG, CHUNK_MAX)
        .map(|c| (c.offset, c.length))
        .collect()
}

pub(super) async fn put_chunk(objects: &dyn ObjectStore, chunk: Bytes) -> Result<ChunkRef> {
    let hash = ObjectHash::digest(ObjectKind::Chunk, &chunk);
    let size = chunk.len() as u32;
    objects.put(ObjectKind::Chunk, &hash, chunk).await?;
    Ok(ChunkRef { hash, size })
}

pub(super) async fn put_manifest(
    objects: &dyn ObjectStore,
    chunks: Vec<ChunkRef>,
) -> Result<ContentRef> {
    let size = chunks.iter().map(|c| c.size as u64).sum();
    let manifest = BlobManifest {
        total_size: size,
        chunks,
    };
    let hash = objects.put_object(&manifest).await?;
    Ok(ContentRef::Chunked { hash, size })
}

#[cfg(test)]
mod tests {
    use super::super::read_content;
    use super::super::testing::pseudo_random;
    use super::*;
    use rill_store::MemoryObjectStore;

    #[tokio::test]
    async fn picks_representation_by_size() {
        let store = MemoryObjectStore::new();
        let small = store_content(&store, Bytes::from_static(b"hi"))
            .await
            .unwrap();
        assert!(small.is_inline());

        let medium = store_content(&store, pseudo_random(4096, 0)).await.unwrap();
        assert!(matches!(medium, ContentRef::Blob { size: 4096, .. }));

        let big_data = pseudo_random(CHUNK_THRESHOLD as usize + 12345, 0);
        let big = store_content(&store, big_data.clone()).await.unwrap();
        assert!(matches!(big, ContentRef::Chunked { .. }));
        assert_eq!(read_content(&store, &big).await.unwrap(), big_data);
        assert_eq!(read_content(&store, &medium).await.unwrap().len(), 4096);
    }

    #[test]
    fn chunks_cover_input() {
        let data = pseudo_random(2 * 1024 * 1024, 0);
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
