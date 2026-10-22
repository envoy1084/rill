//! A version's land-critical values as writes for the transaction that
//! stores it, so a version and what landing needs of it commit together
//! in one transaction instead of one per value.

use std::sync::Arc;

use bytes::Bytes;
use rill_store::repo::encode_tagged;
use rill_store::Mutation;
use rill_types::{ChangeVersion, ChangeVersionHash, EventKind};

use super::ancestry::Ancestry;
use super::changed_paths::ChangedPaths;
use super::manager::DeriveManager;
use super::root_tree::RootTree;
use super::traits::{derived_kind_key, Derivable};
use crate::error::Result;

/// Derived rows to commit with a version, then [`DeriveManager::committed`].
#[derive(Debug, Default)]
pub struct LandCritical {
    pub mutations: Vec<Mutation>,
    values: Vec<(String, ChangeVersionHash, Bytes)>,
}

impl DeriveManager {
    /// The rows of `cv`'s root tree, changed paths and ancestry not stored
    /// yet, computed from its parents' (derived first if they lack them).
    /// `cv` need not be stored; its tree objects are written now.
    pub async fn land_critical_writes(
        self: &Arc<Self>,
        cv: &ChangeVersion,
    ) -> Result<LandCritical> {
        let hash = cv.version_hash();
        let mut out = LandCritical::default();
        self.add::<RootTree>(cv, &hash, &mut out).await?;
        self.add::<ChangedPaths>(cv, &hash, &mut out).await?;
        self.add::<Ancestry>(cv, &hash, &mut out).await?;
        Ok(out)
    }

    /// Note that the rows of `written` committed.
    pub fn committed(&self, written: LandCritical) {
        for (kind, hash, bytes) in written.values {
            self.remember(&kind, &hash, bytes);
        }
    }

    /// Store `cv` with its land-critical values, in one transaction.
    pub async fn store_version(self: &Arc<Self>, cv: &ChangeVersion) -> Result<ChangeVersionHash> {
        let repo = self.repo();
        let hash = cv.version_hash();
        if repo.has_change_version(&hash).await? {
            self.ensure_land_critical(&hash).await?;
            return Ok(hash);
        }
        // The version's object and its trees are written concurrently.
        let (tx, derived) = futures::try_join!(
            async {
                repo.stage_version(cv)
                    .await
                    .map_err(crate::error::Error::from)
            },
            self.land_critical_writes(cv)
        )?;
        let mut tx = tx;
        tx.mutations.extend(derived.mutations.iter().cloned());
        tx.mutations.extend(repo.event_by(
            &cv.author,
            EventKind::VersionStored {
                version: hash,
                change: cv.change_id,
            },
        )?);
        match repo.meta().transact(tx).await {
            Ok(_) => {
                self.committed(derived);
                Ok(hash)
            }
            Err(e) if e.is_conflict() && repo.has_change_version(&hash).await? => {
                self.ensure_land_critical(&hash).await?;
                Ok(hash)
            }
            Err(e) => Err(e.into()),
        }
    }

    async fn add<D: Derivable>(
        self: &Arc<Self>,
        cv: &ChangeVersion,
        hash: &ChangeVersionHash,
        out: &mut LandCritical,
    ) -> Result<()> {
        let kind = derived_kind_key::<D>();
        if self.load::<D>(&kind, hash).await?.is_some() {
            return Ok(());
        }
        let mut parents = Vec::new();
        if D::NEEDS_PARENTS {
            for parent in &cv.parents {
                parents.push(self.get::<D>(parent).await?);
            }
        }
        let value = D::derive(&self.ctx(), cv, parents).await?;
        let bytes = encode_tagged(&kind, &value)?;
        out.mutations.push(
            self.repo()
                .derived_mutation(&kind, hash, bytes.clone())
                .await?,
        );
        out.values.push((kind, *hash, bytes));
        Ok(())
    }
}
