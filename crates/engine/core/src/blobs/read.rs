//! Reading content back, whole, by range or as a stream of parts.

use bytes::{Bytes, BytesMut};
use futures::stream::{self, BoxStream, StreamExt};
use rill_store::{ObjectStore, ObjectStoreExt};
use std::sync::Arc;

use rill_types::{BlobManifest, ContentRef, ObjectHash, ObjectKind};

use crate::error::{Error, Result};

/// Chunk reads kept in flight while streaming, in order.
const CONCURRENT_CHUNKS: usize = 16;

/// Read the bytes behind `content`, reassembling chunks and verifying sizes.
pub async fn read_content(objects: &dyn ObjectStore, content: &ContentRef) -> Result<Bytes> {
    read_content_range(objects, content, 0, u64::MAX).await
}

/// Read up to `len` bytes of `content` from `offset`, fetching only the
/// chunks the range covers.
pub async fn read_content_range(
    objects: &dyn ObjectStore,
    content: &ContentRef,
    offset: u64,
    len: u64,
) -> Result<Bytes> {
    match content {
        ContentRef::Inline { bytes } => Ok(slice(bytes, offset, len)),
        ContentRef::Blob { hash, size } => {
            let bytes = objects.get_required(ObjectKind::Blob, hash).await?;
            check_size(*size, bytes.len() as u64)?;
            Ok(slice(&bytes, offset, len))
        }
        ContentRef::Chunked { hash, size } => {
            let spans = spans(objects, hash, *size, offset, len).await?;
            let keys: Vec<_> = spans.iter().map(|s| (ObjectKind::Chunk, s.hash)).collect();
            let parts = objects.get_many(&keys).await?;
            let mut out = BytesMut::new();
            for (span, part) in spans.iter().zip(parts) {
                let part = part.ok_or(rill_store::Error::ObjectNotFound {
                    kind: ObjectKind::Chunk,
                    hash: span.hash,
                })?;
                out.extend_from_slice(&span.cut(part)?);
            }
            Ok(out.freeze())
        }
    }
}

/// The bytes of `content` from `offset` (at most `len`) as a stream of
/// parts, one chunk at a time, so a large file is never held whole.
pub async fn content_parts(
    objects: Arc<dyn ObjectStore>,
    content: &ContentRef,
    offset: u64,
    len: u64,
) -> Result<BoxStream<'static, Result<Bytes>>> {
    let ContentRef::Chunked { hash, size } = content else {
        let bytes = read_content_range(objects.as_ref(), content, offset, len).await?;
        return Ok(stream::iter([Ok(bytes)]).boxed());
    };
    let spans = spans(objects.as_ref(), hash, *size, offset, len).await?;
    Ok(stream::iter(spans)
        .map(move |span| {
            let objects = objects.clone();
            async move {
                let bytes = objects.get_required(ObjectKind::Chunk, &span.hash).await?;
                span.cut(bytes)
            }
        })
        .buffered(CONCURRENT_CHUNKS)
        .boxed())
}

/// The part of one chunk a range covers.
struct Span {
    hash: ObjectHash,
    size: u32,
    from: usize,
    to: usize,
}

impl Span {
    fn cut(&self, bytes: Bytes) -> Result<Bytes> {
        check_size(self.size as u64, bytes.len() as u64)?;
        Ok(bytes.slice(self.from..self.to))
    }
}

/// The chunks of manifest `hash` that `[offset, offset + len)` covers. The
/// manifest's sizes are checked against each other and `size` before any
/// allocation depends on them.
async fn spans(
    objects: &dyn ObjectStore,
    hash: &ObjectHash,
    size: u64,
    offset: u64,
    len: u64,
) -> Result<Vec<Span>> {
    let manifest: BlobManifest = objects.get_object_required(hash).await?;
    let total: u64 = manifest.chunks.iter().map(|c| c.size as u64).sum();
    check_size(size, total)?;
    check_size(manifest.total_size, total)?;
    let end = offset.saturating_add(len).min(total);
    let mut out = Vec::new();
    let mut start = 0u64;
    for chunk in manifest.chunks {
        let chunk_end = start + chunk.size as u64;
        if chunk_end > offset && start < end {
            out.push(Span {
                hash: chunk.hash,
                size: chunk.size,
                from: offset.saturating_sub(start) as usize,
                to: (end.min(chunk_end) - start) as usize,
            });
        }
        start = chunk_end;
    }
    Ok(out)
}

fn slice(bytes: &Bytes, offset: u64, len: u64) -> Bytes {
    let start = offset.min(bytes.len() as u64) as usize;
    let end = offset.saturating_add(len).min(bytes.len() as u64) as usize;
    bytes.slice(start..end)
}

fn check_size(expected: u64, actual: u64) -> Result<()> {
    if expected != actual {
        return Err(Error::ContentSize { expected, actual });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::super::{pseudo_random, store_content};
    use super::*;
    use async_trait::async_trait;
    use rill_store::MemoryObjectStore;

    /// Counts chunk reads.
    struct Counting {
        inner: MemoryObjectStore,
        chunk_reads: AtomicUsize,
    }

    #[async_trait]
    impl ObjectStore for Counting {
        async fn get(
            &self,
            kind: ObjectKind,
            hash: &ObjectHash,
        ) -> rill_store::Result<Option<Bytes>> {
            if kind == ObjectKind::Chunk {
                self.chunk_reads.fetch_add(1, Ordering::SeqCst);
            }
            self.inner.get(kind, hash).await
        }
        async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> rill_store::Result<bool> {
            self.inner.has(kind, hash).await
        }
        async fn put(
            &self,
            kind: ObjectKind,
            hash: &ObjectHash,
            bytes: Bytes,
        ) -> rill_store::Result<()> {
            self.inner.put(kind, hash, bytes).await
        }
        fn namespaced(&self, _: &str) -> Arc<dyn ObjectStore> {
            unreachable!("the test uses one namespace")
        }
    }

    #[tokio::test]
    async fn range_reads_touch_only_covering_chunks() {
        let store = Arc::new(Counting {
            inner: MemoryObjectStore::new(),
            chunk_reads: AtomicUsize::new(0),
        });
        let data = pseudo_random(16 * 1024 * 1024, 9);
        let content = store_content(store.as_ref(), data.clone()).await.unwrap();

        let mid = 7 * 1024 * 1024 + 123;
        let read = read_content_range(store.as_ref(), &content, mid, 4096)
            .await
            .unwrap();
        assert_eq!(read, data.slice(mid as usize..mid as usize + 4096));
        assert!(store.chunk_reads.load(Ordering::SeqCst) <= 2);

        assert_eq!(read_content(store.as_ref(), &content).await.unwrap(), data);
        let tail = read_content_range(store.as_ref(), &content, data.len() as u64 - 5, 100)
            .await
            .unwrap();
        assert_eq!(tail, data.slice(data.len() - 5..));
    }
}
