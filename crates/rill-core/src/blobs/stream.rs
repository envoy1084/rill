//! Storing content as it arrives.

use std::sync::Arc;

use bytes::{Bytes, BytesMut};
use fastcdc::v2020::AsyncStreamCDC;
use futures::{Stream, StreamExt};
use parking_lot::Mutex;
use rill_store::ObjectStore;
use rill_types::limits::MAX_FILE_BYTES;
use rill_types::objects::blob::CHUNK_THRESHOLD;
use rill_types::{ContentRef, ObjectHash, ObjectKind};
use tokio_util::io::StreamReader;

use super::store::{put_chunk, put_manifest, store_content};
use super::{CHUNK_AVG, CHUNK_MAX, CHUNK_MIN};
use crate::error::{Error, Result};

/// What [`store_content_stream`] stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredContent {
    pub content: ContentRef,
    /// Hash of the whole content as one blob, whatever form it was stored in.
    pub blob_hash: ObjectHash,
    pub size: u64,
}

/// Store content arriving as `parts`, holding at most a chunk or so at a
/// time. The result is the same [`ContentRef`] [`store_content`] gives for
/// the same bytes: content below the chunking threshold is stored whole,
/// and larger content is cut by fastcdc's streaming chunker with the same
/// parameters as the one-shot one.
pub async fn store_content_stream<S>(
    objects: &dyn ObjectStore,
    mut parts: S,
) -> Result<StoredContent>
where
    S: Stream<Item = Result<Bytes>> + Unpin + Send,
{
    let hasher = Arc::new(Mutex::new(ObjectHash::hasher(ObjectKind::Blob)));
    let size = Arc::new(Mutex::new(0u64));
    let failure: Arc<Mutex<Option<Error>>> = Arc::default();
    let count = {
        let (hasher, size) = (hasher.clone(), size.clone());
        move |part: &Bytes| -> Result<()> {
            let mut size = size.lock();
            *size += part.len() as u64;
            if *size > MAX_FILE_BYTES {
                return Err(Error::InvalidChange("content is larger than 5 GiB".into()));
            }
            hasher.lock().update(part);
            Ok(())
        }
    };

    let mut prefix = BytesMut::new();
    while (prefix.len() as u64) < CHUNK_THRESHOLD {
        match parts.next().await {
            Some(part) => {
                let part = part?;
                count(&part)?;
                prefix.extend_from_slice(&part);
            }
            None => {
                let content = store_content(objects, prefix.freeze()).await?;
                let blob_hash =
                    std::mem::replace(&mut *hasher.lock(), ObjectHash::hasher(ObjectKind::Blob))
                        .finalize();
                let size = *size.lock();
                return Ok(StoredContent {
                    content,
                    blob_hash,
                    size,
                });
            }
        }
    }

    // The chunker reads bytes, so errors cross it as I/O errors; the typed
    // error is kept aside and returned instead.
    let rest = {
        let failure = failure.clone();
        parts.map(move |part| {
            let part = part.and_then(|p| count(&p).map(|()| p));
            part.map_err(|e| {
                let message = e.to_string();
                *failure.lock() = Some(e);
                std::io::Error::other(message)
            })
        })
    };
    let input = futures::stream::iter([Ok(prefix.freeze())]).chain(rest);
    let mut chunker =
        AsyncStreamCDC::new(StreamReader::new(input), CHUNK_MIN, CHUNK_AVG, CHUNK_MAX);
    let mut cut = Box::pin(chunker.as_stream());
    let mut chunks = Vec::new();
    while let Some(chunk) = cut.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(e) => {
                return Err(failure
                    .lock()
                    .take()
                    .unwrap_or_else(|| Error::InvalidChange(format!("reading content: {e}"))))
            }
        };
        chunks.push(put_chunk(objects, Bytes::from(chunk.data)).await?);
    }
    let content = put_manifest(objects, chunks).await?;
    let blob_hash =
        std::mem::replace(&mut *hasher.lock(), ObjectHash::hasher(ObjectKind::Blob)).finalize();
    let size = *size.lock();
    Ok(StoredContent {
        content,
        blob_hash,
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
