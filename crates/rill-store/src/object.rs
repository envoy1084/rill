//! The content-addressed object store.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_types::{Encodable, ObjectHash, ObjectKind};

use crate::error::{Error, Result};

/// An object fetched from the store together with its identity.
#[derive(Debug, Clone)]
pub struct StoredObject {
    pub kind: ObjectKind,
    pub hash: ObjectHash,
    pub bytes: Bytes,
}

/// Immutable, content-addressed storage.
///
/// Keys are `(kind, hash)`; values are the raw bytes whose domain-separated
/// BLAKE3 digest is `hash`. Implementations verify the hash on `put` so a
/// corrupt or malicious upload can never be served under a wrong identity.
#[async_trait]
pub trait ObjectStore: Send + Sync + 'static {
    async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>>;

    async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool>;

    /// Store `bytes` under `(kind, hash)`; returns an error if the bytes do
    /// not hash to `hash`. Storing an existing object is a no-op.
    async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()>;

    /// Default batch implementation; backends with a native batch call
    /// override it.
    async fn has_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<bool>> {
        let mut out = Vec::with_capacity(items.len());
        for (kind, hash) in items {
            out.push(self.has(*kind, hash).await?);
        }
        Ok(out)
    }

    async fn get_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<Option<Bytes>>> {
        let mut out = Vec::with_capacity(items.len());
        for (kind, hash) in items {
            out.push(self.get(*kind, hash).await?);
        }
        Ok(out)
    }
}

/// Typed helpers available on every object store.
#[async_trait]
pub trait ObjectStoreExt: ObjectStore {
    async fn get_required(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Bytes> {
        self.get(kind, hash)
            .await?
            .ok_or(Error::ObjectNotFound { kind, hash: *hash })
    }

    /// Store an encodable object and return its hash.
    async fn put_object<T: Encodable + Sync>(&self, value: &T) -> Result<ObjectHash> {
        let bytes = value.encode();
        let hash = ObjectHash::digest(T::KIND, &bytes);
        self.put(T::KIND, &hash, Bytes::from(bytes)).await?;
        Ok(hash)
    }

    async fn get_object<T: Encodable>(&self, hash: &ObjectHash) -> Result<Option<T>> {
        match self.get(T::KIND, hash).await? {
            Some(bytes) => Ok(Some(T::decode(&bytes)?)),
            None => Ok(None),
        }
    }

    async fn get_object_required<T: Encodable>(&self, hash: &ObjectHash) -> Result<T> {
        let bytes = self.get_required(T::KIND, hash).await?;
        Ok(T::decode(&bytes)?)
    }

    /// Store raw file content as a blob and return its hash.
    async fn put_blob(&self, bytes: Bytes) -> Result<ObjectHash> {
        let hash = ObjectHash::digest(ObjectKind::Blob, &bytes);
        self.put(ObjectKind::Blob, &hash, bytes).await?;
        Ok(hash)
    }
}

impl<T: ObjectStore + ?Sized> ObjectStoreExt for T {}

/// Verify that `bytes` hash to `expected` under `kind`.
pub fn verify(kind: ObjectKind, expected: &ObjectHash, bytes: &[u8]) -> Result<()> {
    let actual = if bytes.len() >= 1 << 20 {
        ObjectHash::digest_parallel(kind, bytes)
    } else {
        ObjectHash::digest(kind, bytes)
    };
    if &actual != expected {
        return Err(Error::HashMismatch {
            expected: *expected,
            actual,
        });
    }
    Ok(())
}

#[async_trait]
impl<T: ObjectStore + ?Sized> ObjectStore for Arc<T> {
    async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        (**self).get(kind, hash).await
    }

    async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        (**self).has(kind, hash).await
    }

    async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()> {
        (**self).put(kind, hash, bytes).await
    }

    async fn has_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<bool>> {
        (**self).has_many(items).await
    }

    async fn get_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<Option<Bytes>>> {
        (**self).get_many(items).await
    }
}
