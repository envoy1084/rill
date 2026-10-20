//! Object store over the [`object_store`] crate: S3 and S3-compatible
//! services, plus the in-memory and local-filesystem stores it ships, which
//! make this backend testable without network access.
//!
//! Keys are `<prefix>/<kind>/<hh>/<hash-hex>`, the same shape as the local
//! store, and values are [`codec`](crate::codec) frames. Writes use
//! `PutMode::Create` so an object is never overwritten; a concurrent writer
//! of the same (identical) object is treated as success.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use futures::stream::{self, StreamExt, TryStreamExt};
use object_store::path::Path;
use object_store::{ObjectStoreExt as _, PutMode, PutOptions};
use rill_types::{ObjectHash, ObjectKind};
use url::Url;

use crate::codec;
use crate::error::{Error, Result};
use crate::object::{verify, ObjectStore};

/// How many `has`/`get` requests a batch call keeps in flight.
const BATCH_CONCURRENCY: usize = 16;

/// [`ObjectStore`] over any [`object_store::ObjectStore`].
#[derive(Debug, Clone)]
pub struct ObjectStoreBackend {
    store: Arc<dyn object_store::ObjectStore>,
    prefix: Path,
}

impl ObjectStoreBackend {
    /// Wrap `store`, placing every key under `prefix` (may be empty).
    pub fn new(store: Arc<dyn object_store::ObjectStore>, prefix: impl AsRef<str>) -> Self {
        ObjectStoreBackend {
            store,
            prefix: Path::from(prefix.as_ref()),
        }
    }

    /// Build a backend from a URL such as `s3://bucket/path`, `file:///dir`
    /// or `memory:///`. The URL's own path and `prefix` are joined.
    /// Credentials and region for S3 come from the environment, as
    /// `object_store::parse_url` documents.
    pub fn from_url(url: &str, prefix: impl AsRef<str>) -> Result<Self> {
        let url =
            Url::parse(url).map_err(|e| Error::Backend(format!("bad object store url: {e}")))?;
        let (store, url_path) = object_store::parse_url(&url).map_err(Error::backend)?;
        let mut full = url_path;
        for part in Path::from(prefix.as_ref()).parts() {
            full = full.join(part);
        }
        Ok(ObjectStoreBackend {
            store: Arc::from(store),
            prefix: full,
        })
    }

    /// Key prefix every object lives under.
    pub fn prefix(&self) -> &Path {
        &self.prefix
    }

    fn path_for(&self, kind: ObjectKind, hash: &ObjectHash) -> Path {
        let hex = hash.to_hex();
        self.prefix
            .clone()
            .join(kind.as_str())
            .join(&hex[..2])
            .join(hex.as_str())
    }

    async fn fetch(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        let path = self.path_for(kind, hash);
        match self.store.get(&path).await {
            Ok(result) => {
                let frame = result.bytes().await.map_err(Error::backend)?;
                // Decompression is CPU work proportional to the object; keep
                // it off the reactor for anything larger than a tree node.
                let decoded = if frame.len() > 64 * 1024 {
                    tokio::task::spawn_blocking(move || codec::decompress(&frame))
                        .await
                        .map_err(Error::backend)??
                } else {
                    codec::decompress(&frame)?
                };
                Ok(Some(Bytes::from(decoded)))
            }
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(Error::backend(e)),
        }
    }

    async fn exists(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        let path = self.path_for(kind, hash);
        match self.store.head(&path).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(e) => Err(Error::backend(e)),
        }
    }
}

#[async_trait]
impl ObjectStore for ObjectStoreBackend {
    async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        self.fetch(kind, hash).await
    }

    async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        self.exists(kind, hash).await
    }

    async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()> {
        let expected = *hash;
        let frame = tokio::task::spawn_blocking(move || {
            verify(kind, &expected, &bytes)?;
            Ok::<_, Error>(Bytes::from(codec::compress(&bytes)))
        })
        .await
        .map_err(Error::backend)??;

        let path = self.path_for(kind, hash);
        let opts = PutOptions::from(PutMode::Create);
        match self.store.put_opts(&path, frame.into(), opts).await {
            Ok(_) => Ok(()),
            // Content-addressed: whoever got there first wrote the same bytes.
            Err(object_store::Error::AlreadyExists { .. }) => Ok(()),
            Err(e) => Err(Error::backend(e)),
        }
    }

    async fn has_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<bool>> {
        stream::iter(items.iter().copied())
            .map(|(kind, hash)| async move { self.exists(kind, &hash).await })
            .buffered(BATCH_CONCURRENCY)
            .try_collect()
            .await
    }

    async fn get_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<Option<Bytes>>> {
        stream::iter(items.iter().copied())
            .map(|(kind, hash)| async move { self.fetch(kind, &hash).await })
            .buffered(BATCH_CONCURRENCY)
            .try_collect()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use object_store::memory::InMemory;

    fn backend() -> ObjectStoreBackend {
        ObjectStoreBackend::new(Arc::new(InMemory::new()), "rill/objects")
    }

    #[tokio::test]
    async fn keys_are_prefixed_and_sharded() {
        let b = backend();
        let hash = ObjectHash::digest(ObjectKind::Blob, b"x");
        let hex = hash.to_hex();
        assert_eq!(
            b.path_for(ObjectKind::Blob, &hash).as_ref(),
            format!("rill/objects/blob/{}/{}", &hex[..2], hex)
        );
    }

    #[tokio::test]
    async fn roundtrip_and_batches() {
        let b = backend();
        let bytes = Bytes::from(vec![7u8; 2048]);
        let hash = ObjectHash::digest(ObjectKind::Chunk, &bytes);
        assert!(!b.has(ObjectKind::Chunk, &hash).await.unwrap());
        b.put(ObjectKind::Chunk, &hash, bytes.clone())
            .await
            .unwrap();
        // Second put of the same object hits AlreadyExists and succeeds.
        b.put(ObjectKind::Chunk, &hash, bytes.clone())
            .await
            .unwrap();
        assert_eq!(
            b.get(ObjectKind::Chunk, &hash).await.unwrap(),
            Some(bytes.clone())
        );
        let other = ObjectHash::digest(ObjectKind::Chunk, b"absent");
        assert_eq!(
            b.has_many(&[(ObjectKind::Chunk, hash), (ObjectKind::Chunk, other)])
                .await
                .unwrap(),
            vec![true, false]
        );
        assert_eq!(
            b.get_many(&[(ObjectKind::Chunk, other), (ObjectKind::Chunk, hash)])
                .await
                .unwrap(),
            vec![None, Some(bytes)]
        );
    }

    #[tokio::test]
    async fn rejects_hash_mismatch() {
        let b = backend();
        let wrong = ObjectHash::digest(ObjectKind::Blob, b"other");
        let err = b
            .put(ObjectKind::Blob, &wrong, Bytes::from_static(b"data"))
            .await
            .unwrap_err();
        assert!(matches!(err, Error::HashMismatch { .. }));
        assert!(!b.has(ObjectKind::Blob, &wrong).await.unwrap());
    }

    #[tokio::test]
    async fn from_url_memory_and_file() {
        let mem = ObjectStoreBackend::from_url("memory:///", "p/q").unwrap();
        assert_eq!(mem.prefix().as_ref(), "p/q");
        let bytes = Bytes::from_static(b"hello");
        let hash = ObjectHash::digest(ObjectKind::Blob, &bytes);
        mem.put(ObjectKind::Blob, &hash, bytes.clone())
            .await
            .unwrap();
        assert_eq!(
            mem.get(ObjectKind::Blob, &hash).await.unwrap(),
            Some(bytes.clone())
        );

        let dir = tempfile::tempdir().unwrap();
        let url = Url::from_directory_path(dir.path()).unwrap();
        let fs = ObjectStoreBackend::from_url(url.as_str(), "objs").unwrap();
        fs.put(ObjectKind::Blob, &hash, bytes.clone())
            .await
            .unwrap();
        assert_eq!(fs.get(ObjectKind::Blob, &hash).await.unwrap(), Some(bytes));
        let hex = hash.to_hex();
        assert!(dir
            .path()
            .join("objs")
            .join("blob")
            .join(&hex[..2])
            .join(&hex)
            .is_file());
    }

    #[test]
    fn from_url_rejects_garbage() {
        assert!(ObjectStoreBackend::from_url("not a url", "").is_err());
    }
}
