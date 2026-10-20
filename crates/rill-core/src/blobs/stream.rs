//! Storing content as it arrives.

use bytes::{Bytes, BytesMut};
use futures::{Stream, StreamExt};
use rill_store::ObjectStore;
use rill_types::limits::MAX_FILE_BYTES;
use rill_types::objects::blob::CHUNK_THRESHOLD;
use rill_types::{ContentRef, ObjectHash, ObjectKind};

use super::store::{chunk_boundaries, put_chunk, put_manifest, store_content};
use super::CHUNK_MAX;
use crate::error::{Error, Result};

/// Buffered bytes that trigger chunking a stream's prefix.
const FLUSH_AT: usize = 8 * CHUNK_MAX;

/// What [`store_content_stream`] stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredContent {
    pub content: ContentRef,
    /// Hash of the whole content as one blob, whatever form it was stored in.
    pub blob_hash: ObjectHash,
    pub size: u64,
}

/// Store content arriving as `parts`, holding at most a few megabytes at a
/// time. The result is the same [`ContentRef`] [`store_content`] gives for
/// the same bytes.
///
/// A FastCDC cut depends only on the bytes from the chunk's start to at most
/// [`CHUNK_MAX`] past it, so a chunk whose start is at least `CHUNK_MAX` from
/// the end of the buffer is final and can be written before the rest of the
/// stream arrives.
pub async fn store_content_stream<S>(
    objects: &dyn ObjectStore,
    mut parts: S,
) -> Result<StoredContent>
where
    S: Stream<Item = Result<Bytes>> + Unpin + Send,
{
    let mut hasher = ObjectHash::hasher(ObjectKind::Blob);
    let mut size: u64 = 0;
    let mut buf = BytesMut::new();
    let mut add = |part: &Bytes, buf: &mut BytesMut| -> Result<()> {
        size += part.len() as u64;
        if size > MAX_FILE_BYTES {
            return Err(Error::InvalidChange("content is larger than 5 GiB".into()));
        }
        hasher.update(part);
        buf.extend_from_slice(part);
        Ok(())
    };

    while (buf.len() as u64) < CHUNK_THRESHOLD {
        match parts.next().await {
            Some(part) => add(&part?, &mut buf)?,
            None => {
                let content = store_content(objects, buf.freeze()).await?;
                return Ok(StoredContent {
                    content,
                    blob_hash: hasher.finalize(),
                    size,
                });
            }
        }
    }

    let mut chunks = Vec::new();
    loop {
        let next = parts.next().await;
        let done = next.is_none();
        if let Some(part) = next {
            add(&part?, &mut buf)?;
        }
        if !done && buf.len() < FLUSH_AT {
            continue;
        }
        let data = buf.split().freeze();
        let scan = data.clone();
        let cuts = tokio::task::spawn_blocking(move || chunk_boundaries(&scan)).await?;
        let mut consumed = 0;
        for (offset, len) in cuts {
            if !done && offset + CHUNK_MAX > data.len() {
                break;
            }
            chunks.push(put_chunk(objects, data.slice(offset..offset + len)).await?);
            consumed = offset + len;
        }
        buf.extend_from_slice(&data[consumed..]);
        if done {
            break;
        }
    }
    let content = put_manifest(objects, chunks).await?;
    Ok(StoredContent {
        content,
        blob_hash: hasher.finalize(),
        size,
    })
}

#[cfg(test)]
mod tests {
    use super::super::pseudo_random;
    use super::*;
    use rill_store::MemoryObjectStore;

    async fn streamed(store: &MemoryObjectStore, data: &Bytes, piece: usize) -> StoredContent {
        let parts: Vec<Result<Bytes>> = (0..data.len())
            .step_by(piece)
            .map(|i| Ok(data.slice(i..(i + piece).min(data.len()))))
            .collect();
        store_content_stream(store, futures::stream::iter(parts))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn streaming_matches_storing_whole_content() {
        let store = MemoryObjectStore::new();
        for (len, piece) in [
            (10, 3),
            (5_000, 777),
            (CHUNK_THRESHOLD as usize - 1, 64 * 1024),
            (CHUNK_THRESHOLD as usize, 100_000),
            (5 * 1024 * 1024 + 17, 1),
            (9 * 1024 * 1024 + 3, 256 * 1024),
        ] {
            let piece = if piece == 1 { 4093 } else { piece };
            let data = pseudo_random(len, len as u64);
            let whole = store_content(&store, data.clone()).await.unwrap();
            let stored = streamed(&store, &data, piece).await;
            assert_eq!(stored.content, whole, "len {len}, piece {piece}");
            assert_eq!(stored.size, len as u64);
            assert_eq!(
                stored.blob_hash,
                ObjectHash::digest(ObjectKind::Blob, &data)
            );
        }
    }
}
