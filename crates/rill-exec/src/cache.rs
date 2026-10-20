//! The action cache: results keyed by action digest.
//!
//! Results live in the repository's derived-data keyspace, so every worker
//! sharing the metadata store shares the cache, and stdout/stderr are
//! blobs in the object store like any other content.

use bytes::Bytes;
use rill_core::store_content;
use rill_store::repo::{decode_tagged, encode_tagged};
use rill_store::RepoStore;
use rill_types::{ContentRef, ObjectHash};

use crate::action::ActionResult;
use crate::error::{Error, Result};

/// Derived-data kind under which results are stored.
pub const CACHE_KIND: &str = "ac@1";

/// Content-addressed cache of [`ActionResult`]s.
#[derive(Clone)]
pub struct ActionCache {
    repo: RepoStore,
}

impl ActionCache {
    /// A cache over `repo`'s derived-data keyspace.
    pub fn new(repo: RepoStore) -> Self {
        ActionCache { repo }
    }

    /// The repository backing this cache.
    pub fn repo(&self) -> &RepoStore {
        &self.repo
    }

    /// The cached result for `digest`, if any.
    pub async fn get(&self, digest: &ObjectHash) -> Result<Option<ActionResult>> {
        match self.repo.get_derived(CACHE_KIND, digest).await? {
            Some(bytes) => Ok(Some(
                decode_tagged(CACHE_KIND, &bytes).map_err(|e| Error::Codec(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    /// Store `result` under `digest`, replacing any previous entry.
    pub async fn put(&self, digest: &ObjectHash, result: &ActionResult) -> Result<()> {
        let bytes = encode_tagged(CACHE_KIND, result).map_err(|e| Error::Codec(e.to_string()))?;
        self.repo.put_derived(CACHE_KIND, digest, bytes).await?;
        Ok(())
    }

    /// Store captured output bytes and return the reference to put in a
    /// result.
    pub async fn store_output(&self, bytes: Bytes) -> Result<ContentRef> {
        Ok(store_content(self.repo.objects().as_ref(), bytes).await?)
    }

    /// Read back output stored with [`ActionCache::store_output`].
    pub async fn read_output(&self, content: &ContentRef) -> Result<Bytes> {
        Ok(rill_core::read_content(self.repo.objects().as_ref(), content).await?)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rill_store::Stores;
    use rill_types::Timestamp;

    use super::*;

    #[tokio::test]
    async fn roundtrip() {
        let stores = Stores::memory();
        let config = stores.create_repo("r", "").await.unwrap();
        let cache = ActionCache::new(stores.repo(config.id));
        let digest = ObjectHash::digest(rill_types::ObjectKind::Derived, b"k");
        assert!(cache.get(&digest).await.unwrap().is_none());

        let stdout = cache.store_output(Bytes::from_static(b"hi")).await.unwrap();
        let result = ActionResult {
            exit_code: 0,
            stdout: stdout.clone(),
            stderr: ContentRef::inline(Vec::new()),
            duration_ms: 3,
            output_files: BTreeMap::new(),
            worker: "test".into(),
            started_at: Timestamp::from_millis(1),
            timed_out: false,
        };
        cache.put(&digest, &result).await.unwrap();
        assert_eq!(cache.get(&digest).await.unwrap(), Some(result));
        assert_eq!(cache.read_output(&stdout).await.unwrap().as_ref(), b"hi");
    }
}
