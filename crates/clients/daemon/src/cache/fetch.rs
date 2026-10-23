//! Reading content through the cache: inline content directly, stored
//! content from disk or, on a miss, from the server, verified and kept.

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
    /// Chunked content's manifest hash → the digest of its bytes.
    digests: moka::sync::Cache<ObjectHash, ObjectHash>,
}

impl Content {
    /// Content access over `cache`.
    pub fn new(cache: Arc<ObjectCache>) -> Self {
        Content {
            cache,
            digests: moka::sync::Cache::new(1 << 20),
        }
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
        if let Some(bytes) = tokio::task::spawn_blocking(move || cache.get(&key)).await?? {
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
        self.store(key, bytes.clone()).await?;
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
        self.store(digest, bytes).await?;
        Ok(digest)
    }

    /// Locally written bytes by key.
    pub async fn local(&self, digest: ObjectHash) -> Result<Bytes> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.get(&digest))
            .await??
            .ok_or_else(|| Error::NotFound(format!("local content {digest} is not cached")))
    }

    async fn store(&self, key: ObjectHash, bytes: Bytes) -> Result<()> {
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || cache.put(&key, &bytes)).await?
    }
}
