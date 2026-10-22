//! Where an import writes: the store directly, or a server through its API
//! (the CLI's online import). The importer only needs these operations.

use async_trait::async_trait;
use bytes::Bytes;
use rill_core::{store_content, validate_version, PathRules};
use rill_store::RepoStore;
use rill_types::{ChangeVersion, ChangeVersionHash, ContentRef, TrunkSeq};

use super::map::ImportMap;
use crate::error::Result;

/// A repository an import writes into.
#[async_trait]
pub trait ImportTarget: Send + Sync {
    /// Store file content and return its reference.
    async fn store_content(&self, bytes: Bytes) -> Result<ContentRef>;

    /// Store a version (idempotently) and return its hash.
    async fn store_version(&self, version: &ChangeVersion) -> Result<ChangeVersionHash>;

    async fn trunk_head(&self) -> Result<Option<(TrunkSeq, ChangeVersionHash)>>;

    async fn trunk_seq_of(&self, version: &ChangeVersionHash) -> Result<Option<TrunkSeq>>;

    /// Append `version` to trunk if trunk still ends at `expected`.
    async fn append_trunk(
        &self,
        expected: Option<ChangeVersionHash>,
        version: ChangeVersionHash,
    ) -> Result<TrunkSeq>;

    /// The sha mapping saved by an earlier import, or an empty one.
    async fn load_map(&self) -> Result<ImportMap>;

    async fn save_map(&self, map: &ImportMap) -> Result<()>;
}

#[async_trait]
impl ImportTarget for RepoStore {
    async fn store_content(&self, bytes: Bytes) -> Result<ContentRef> {
        Ok(store_content(self.objects().as_ref(), bytes).await?)
    }

    async fn store_version(&self, version: &ChangeVersion) -> Result<ChangeVersionHash> {
        validate_version(self.objects().as_ref(), version, PathRules::default()).await?;
        Ok(self.put_change_version(version).await?)
    }

    async fn trunk_head(&self) -> Result<Option<(TrunkSeq, ChangeVersionHash)>> {
        Ok(RepoStore::trunk_head(self).await?)
    }

    async fn trunk_seq_of(&self, version: &ChangeVersionHash) -> Result<Option<TrunkSeq>> {
        Ok(RepoStore::trunk_seq_of(self, version).await?)
    }

    async fn append_trunk(
        &self,
        expected: Option<ChangeVersionHash>,
        version: ChangeVersionHash,
    ) -> Result<TrunkSeq> {
        Ok(self.land_on_trunk(expected, version).await?)
    }

    async fn load_map(&self) -> Result<ImportMap> {
        ImportMap::load(self).await
    }

    async fn save_map(&self, map: &ImportMap) -> Result<()> {
        map.save(self).await
    }
}
