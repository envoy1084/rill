//! The content-addressable store over an execution space. Content below
//! the chunking threshold is one `Blob` object; larger content is chunked
//! like repository content and found through an index from its digest.

use bytes::Bytes;
use futures::{Stream, StreamExt};
use rill_core::blobs::{read_content, read_content_range, store_content_stream};
use rill_reapi_proto::reapi::{Digest, Directory};
use rill_types::objects::blob::CHUNK_THRESHOLD;
use rill_types::{ContentRef, ObjectKind};

use super::digest::BlobDigest;
use super::space::ExecSpace;
use crate::error::{Error, Result};

/// Derived kind indexing chunked content by its digest.
const LARGE_INDEX: &str = "reapi:large";

/// Where chunked content with a given digest is stored.
#[derive(serde::Serialize, serde::Deserialize)]
struct LargeContent {
    content: ContentRef,
}

impl rill_types::Record for LargeContent {
    const TAG: &'static str = "exec-large";
}

impl ExecSpace {
    async fn large(&self, digest: &BlobDigest) -> Result<Option<ContentRef>> {
        let Some(bytes) = self.records.get_derived(LARGE_INDEX, &digest.hash).await? else {
            return Ok(None);
        };
        let large: LargeContent = rill_types::decode_record(&bytes)?;
        Ok(Some(large.content))
    }

    /// Which of `digests` the store lacks.
    pub async fn missing(&self, digests: &[BlobDigest]) -> Result<Vec<BlobDigest>> {
        let small: Vec<_> = digests
            .iter()
            .filter(|d| !d.is_empty() && d.size < CHUNK_THRESHOLD)
            .map(|d| (ObjectKind::Blob, d.hash))
            .collect();
        let present = self.objects.has_many(&small).await?;
        let mut small_present = present.into_iter();
        let mut missing = Vec::new();
        for d in digests {
            if d.is_empty() {
                continue;
            }
            let found = if d.size < CHUNK_THRESHOLD {
                small_present.next().unwrap_or(false)
            } else {
                self.large(d).await?.is_some()
            };
            if !found {
                missing.push(*d);
            }
        }
        Ok(missing)
    }

    /// The content `digest` names, if stored.
    pub async fn read(&self, digest: &BlobDigest) -> Result<Option<Bytes>> {
        if digest.is_empty() {
            return Ok(Some(Bytes::new()));
        }
        if digest.size < CHUNK_THRESHOLD {
            return Ok(self.objects.get(ObjectKind::Blob, &digest.hash).await?);
        }
        match self.large(digest).await? {
            Some(content) => Ok(Some(read_content(self.objects.as_ref(), &content).await?)),
            None => Ok(None),
        }
    }

    /// `len` bytes of `digest`'s content from `offset`.
    pub async fn read_range(
        &self,
        digest: &BlobDigest,
        offset: u64,
        len: u64,
    ) -> Result<Option<Bytes>> {
        if digest.size < CHUNK_THRESHOLD {
            return Ok(self.read(digest).await?.map(|b| {
                let start = (offset as usize).min(b.len());
                let end = start.saturating_add(len as usize).min(b.len());
                b.slice(start..end)
            }));
        }
        match self.large(digest).await? {
            Some(content) => Ok(Some(
                read_content_range(self.objects.as_ref(), &content, offset, len).await?,
            )),
            None => Ok(None),
        }
    }

    /// Store `bytes` as the content `digest` names; a mismatch is refused.
    pub async fn write(&self, digest: &BlobDigest, bytes: Bytes) -> Result<()> {
        if digest.size < CHUNK_THRESHOLD {
            return self.put_whole(digest, bytes).await;
        }
        digest.verify(&bytes)?;
        self.put_chunked(digest, futures::stream::iter([Ok(bytes)]))
            .await
    }

    /// Store content arriving as `parts`, checked against `digest` once it
    /// is complete. Content below the chunking threshold is collected and
    /// stored as one blob (repository storage would inline the smallest).
    pub async fn write_stream<S>(&self, digest: &BlobDigest, mut parts: S) -> Result<()>
    where
        S: Stream<Item = rill_core::Result<Bytes>> + Unpin + Send,
    {
        if digest.size >= CHUNK_THRESHOLD {
            return self.put_chunked(digest, parts).await;
        }
        let mut whole = bytes::BytesMut::with_capacity(digest.size as usize);
        while let Some(part) = parts.next().await {
            whole.extend_from_slice(&part?);
            if whole.len() as u64 > digest.size {
                return Err(Error::DigestMismatch(digest.to_string()));
            }
        }
        self.put_whole(digest, whole.freeze()).await
    }

    async fn put_whole(&self, digest: &BlobDigest, bytes: Bytes) -> Result<()> {
        digest.verify(&bytes)?;
        if !digest.is_empty() {
            self.objects
                .put(ObjectKind::Blob, &digest.hash, bytes)
                .await?;
        }
        Ok(())
    }

    async fn put_chunked<S>(&self, digest: &BlobDigest, parts: S) -> Result<()>
    where
        S: Stream<Item = rill_core::Result<Bytes>> + Unpin + Send,
    {
        let stored = store_content_stream(self.objects.as_ref(), parts).await?;
        if stored.blob_hash != digest.hash || stored.size != digest.size {
            return Err(Error::DigestMismatch(digest.to_string()));
        }
        if let ContentRef::Chunked { .. } = stored.content {
            self.records
                .put_derived(
                    LARGE_INDEX,
                    &digest.hash,
                    Bytes::from(rill_types::encode_record(&LargeContent {
                        content: stored.content,
                    })),
                )
                .await?;
        }
        Ok(())
    }

    /// Every directory under `root`, root first, each once.
    pub async fn tree(&self, root: &Digest) -> Result<Vec<Directory>> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut pending = vec![BlobDigest::parse(root)?];
        while let Some(digest) = pending.pop() {
            if !seen.insert(digest) {
                continue;
            }
            let bytes = self
                .read(&digest)
                .await?
                .ok_or_else(|| Error::MissingBlob(digest.to_string()))?;
            let dir = <Directory as prost::Message>::decode(bytes)
                .map_err(|e| Error::Codec(format!("directory {digest}: {e}")))?;
            for child in &dir.directories {
                if let Some(d) = &child.digest {
                    pending.push(BlobDigest::parse(d)?);
                }
            }
            out.push(dir);
        }
        Ok(out)
    }
}
