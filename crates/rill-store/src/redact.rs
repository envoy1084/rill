//! Refusing redacted objects. Wrapped around an org's object store, so
//! every read on the server (blobs, diffs, blame, search, the object
//! protocol) fails closed for a redacted object, and its content cannot be
//! stored again.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_types::{ObjectHash, ObjectKind};

use crate::error::{Error, Result};
use crate::object::ObjectStore;

/// Which objects are redacted, by org.
pub trait RedactionSet: Send + Sync + 'static {
    fn is_redacted(&self, org: &str, hash: &ObjectHash) -> bool;
}

/// An org's object store that refuses its redacted objects.
pub struct RedactingObjectStore {
    inner: Arc<dyn ObjectStore>,
    org: Arc<str>,
    redactions: Arc<dyn RedactionSet>,
}

impl RedactingObjectStore {
    pub fn new(inner: Arc<dyn ObjectStore>, org: &str, redactions: Arc<dyn RedactionSet>) -> Self {
        RedactingObjectStore {
            inner,
            org: Arc::from(org),
            redactions,
        }
    }

    fn check(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<()> {
        if self.redactions.is_redacted(&self.org, hash) {
            Err(Error::Redacted { kind, hash: *hash })
        } else {
            Ok(())
        }
    }
}

#[async_trait]
impl ObjectStore for RedactingObjectStore {
    async fn get(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<Option<Bytes>> {
        self.check(kind, hash)?;
        self.inner.get(kind, hash).await
    }

    async fn has(&self, kind: ObjectKind, hash: &ObjectHash) -> Result<bool> {
        self.inner.has(kind, hash).await
    }

    async fn put(&self, kind: ObjectKind, hash: &ObjectHash, bytes: Bytes) -> Result<()> {
        self.check(kind, hash)?;
        self.inner.put(kind, hash, bytes).await
    }

    fn namespaced(&self, namespace: &str) -> Arc<dyn ObjectStore> {
        Arc::new(RedactingObjectStore::new(
            self.inner.namespaced(namespace),
            namespace,
            self.redactions.clone(),
        ))
    }

    async fn has_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<bool>> {
        self.inner.has_many(items).await
    }

    async fn get_many(&self, items: &[(ObjectKind, ObjectHash)]) -> Result<Vec<Option<Bytes>>> {
        for (kind, hash) in items {
            self.check(*kind, hash)?;
        }
        self.inner.get_many(items).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::MemoryObjectStore;

    struct Set(HashSet<(String, ObjectHash)>);

    impl RedactionSet for Set {
        fn is_redacted(&self, org: &str, hash: &ObjectHash) -> bool {
            self.0.contains(&(org.to_string(), *hash))
        }
    }

    #[tokio::test]
    async fn a_redacted_object_is_neither_read_nor_stored_again_in_its_org() {
        let secret = Bytes::from_static(b"the secret");
        let hash = ObjectHash::digest(ObjectKind::Blob, &secret);
        let memory: Arc<dyn ObjectStore> = Arc::new(MemoryObjectStore::new());
        memory
            .put(ObjectKind::Blob, &hash, secret.clone())
            .await
            .unwrap();
        let set = Arc::new(Set(HashSet::from([("default".to_string(), hash)])));
        let store = RedactingObjectStore::new(memory.clone(), "default", set);
        assert!(matches!(
            store.get(ObjectKind::Blob, &hash).await,
            Err(Error::Redacted { .. })
        ));
        assert!(store
            .put(ObjectKind::Blob, &hash, secret.clone())
            .await
            .is_err());
        assert!(store.get_many(&[(ObjectKind::Blob, hash)]).await.is_err());
        let other = store.namespaced("acme");
        other
            .put(ObjectKind::Blob, &hash, secret.clone())
            .await
            .unwrap();
        assert_eq!(
            other.get(ObjectKind::Blob, &hash).await.unwrap(),
            Some(secret)
        );
    }
}
