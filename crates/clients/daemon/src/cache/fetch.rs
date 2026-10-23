//! Reading content through the cache: inline content directly, stored
//! content from disk or, on a miss, from the server, verified and kept.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use rill_sdk::RepoClient;
use rill_types::{ContentRef, ObjectHash, ObjectKind};

use super::store::ObjectCache;
use crate::error::{Error, Result};

/// The key local content is cached under: its BLAKE3 blob digest, which is
/// also the server's hash for inline and unchunked content.
pub fn local_digest(bytes: &[u8]) -> ObjectHash {
    ObjectHash::digest(ObjectKind::Blob, bytes)
}

/// Content access shared by every mount.
#[derive(Clone, Debug)]
pub struct Content {
    cache: Arc<ObjectCache>,
    /// Latency of reads the cache served, in microseconds.
    hits: Arc<parking_lot::Mutex<hdrhistogram::Histogram<u64>>>,
    /// Chunked content's manifest hash → the digest of its bytes.
    digests: moka::sync::Cache<ObjectHash, ObjectHash>,
    /// One fetch per object at a time: concurrent readers of a missing
    /// object wait for the first one's fetch.
    inflight: Arc<parking_lot::Mutex<HashMap<ObjectHash, Arc<tokio::sync::Mutex<()>>>>>,
}

impl Content {
    /// Content access over `cache`.
    pub fn new(cache: Arc<ObjectCache>) -> Self {
        Content {
            cache,
            hits: Arc::new(parking_lot::Mutex::new(
                hdrhistogram::Histogram::new_with_bounds(1, 600_000_000, 2)
                    .expect("static bounds are valid"),
            )),
            digests: moka::sync::Cache::new(1 << 20),
            inflight: Arc::default(),
        }
    }

    /// Reads the cache served: count, median and 99th percentile.
    pub fn hit_latency(&self) -> (u64, std::time::Duration, std::time::Duration) {
        let h = self.hits.lock();
        let at = |q| std::time::Duration::from_micros(h.value_at_quantile(q));
        (h.len(), at(0.5), at(0.99))
    }

    /// The cache underneath.
    pub fn cache(&self) -> &Arc<ObjectCache> {
        &self.cache
    }

    /// The bytes behind `content`, fetched from `repo` on a miss.
    pub async fn get(&self, repo: &RepoClient, content: &ContentRef) -> Result<Bytes> {
        if let ContentRef::Inline { bytes } = content {
            return Ok(bytes.clone());
        }
        let key = content.content_hash();
        let cache = self.cache.clone();
        let started = std::time::Instant::now();
        if let Some(bytes) = tokio::task::spawn_blocking(move || cache.get(&key)).await?? {
            let micros = (started.elapsed().as_micros() as u64).clamp(1, 600_000_000);
            let _ = self.hits.lock().record(micros);
            return Ok(bytes);
        }
        let gate = self.inflight.lock().entry(key).or_default().clone();
        let _turn = gate.lock().await;
        let fetched = self.fetch(repo, content, key).await;
        self.inflight.lock().remove(&key);
        fetched
    }

    /// Fetch `content` (the caller holds its in-flight turn), unless an
    /// earlier turn already cached it.
    async fn fetch(
        &self,
        repo: &RepoClient,
        content: &ContentRef,
        key: ObjectHash,
    ) -> Result<Bytes> {
        let cache = self.cache.clone();
        if let Some(bytes) = tokio::task::spawn_blocking(move || cache.peek(&key)).await?? {
            return Ok(bytes);
        }
        let bytes = repo.get_blob(content).await?;
        let digest = local_digest(&bytes);
        match content {
            ContentRef::Blob { hash, .. } if *hash != digest => {
                return Err(Error::Invalid(format!(
                    "blob {hash} arrived with content {digest}"
                )));
            }
            ContentRef::Chunked { hash, .. } => self.digests.insert(*hash, digest),
            _ => {}
        }
        self.store(key, bytes.clone(), false).await?;
        Ok(bytes)
    }

    /// The digest of the bytes behind `content` (see [`local_digest`]),
    /// reading chunked content once to learn it.
    pub async fn digest(&self, repo: &RepoClient, content: &ContentRef) -> Result<ObjectHash> {
        match content {
            ContentRef::Chunked { hash, .. } => match self.digests.get(hash) {
                Some(d) => Ok(d),
                None => {
                    let bytes = self.get(repo, content).await?;
                    let digest = local_digest(&bytes);
                    self.digests.insert(*hash, digest);
                    Ok(digest)
                }
            },
            other => Ok(other.content_hash()),
        }
    }

    /// Keep locally written bytes and return their key.
    pub async fn put_local(&self, bytes: Bytes) -> Result<ObjectHash> {
        let digest = local_digest(&bytes);
        self.store(digest, bytes, true).await?;
        Ok(digest)
    }

    /// Locally written bytes by key.
    pub async fn local(&self, digest: ObjectHash) -> Result<Bytes> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.get(&digest))
            .await??
            .ok_or_else(|| Error::NotFound(format!("local content {digest} is not cached")))
    }

    async fn store(&self, key: ObjectHash, bytes: Bytes, durable: bool) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.put(&key, &bytes, durable)).await?
    }
}
