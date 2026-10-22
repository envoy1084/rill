//! A version's land-critical values as writes for the transaction that
//! stores it, so a version and what landing needs of it commit together
//! in one transaction instead of one per value.

use std::collections::HashMap;
use std::sync::Arc;

use bytes::Bytes;
use rill_store::repo::encode_tagged;
use rill_store::Mutation;
use rill_types::{ChangeVersion, ChangeVersionHash, EventKind, TreeHash};

use super::ancestry::{next_ancestry, Ancestry, AncestryInfo};
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

    /// Store `chain`, versions each based on the one before it (the first
    /// on a stored version), with their land-critical values: the values
    /// computed in order in memory, every object written at once, and
    /// one transaction for all of it. Falls back to one transaction per
    /// version when some are stored already.
    pub async fn store_chain(
        self: &Arc<Self>,
        chain: &[ChangeVersion],
    ) -> Result<Vec<ChangeVersionHash>> {
        let repo = self.repo();
        let hashes: Vec<ChangeVersionHash> = chain.iter().map(|cv| cv.version_hash()).collect();
        let trees = self.trees().deferring();
        let ctx = super::ctx::DeriveCtx::new(repo.clone(), trees.clone(), self.clone());
        let mut roots: HashMap<ChangeVersionHash, TreeHash> = HashMap::new();
        let mut ancestry: HashMap<ChangeVersionHash, AncestryInfo> = HashMap::new();
        let mut written = LandCritical::default();
        for (cv, hash) in chain.iter().zip(&hashes) {
            let mut parent_roots = Vec::new();
            let mut parent_ancestry = Vec::new();
            for parent in &cv.parents {
                parent_roots.push(match roots.get(parent) {
                    Some(root) => *root,
                    None => self.get::<RootTree>(parent).await?,
                });
                parent_ancestry.push(match ancestry.get(parent) {
                    Some(info) => *info,
                    None => self.get::<Ancestry>(parent).await?,
                });
            }
            let root = RootTree::derive(&ctx, cv, parent_roots).await?;
            let jump_of_jump = match parent_ancestry.first() {
                Some(first) => Some(match ancestry.get(&first.jump) {
                    Some(info) => *info,
                    None => self.get::<Ancestry>(&first.jump).await?,
                }),
                None => None,
            };
            let info = next_ancestry(cv, &parent_ancestry, jump_of_jump);
            let changed = ChangedPaths::derive(&ctx, cv, Vec::new()).await?;
            roots.insert(*hash, root);
            ancestry.insert(*hash, info);
            self.row::<RootTree>(hash, &root, &mut written).await?;
            self.row::<ChangedPaths>(hash, &changed, &mut written)
                .await?;
            self.row::<Ancestry>(hash, &info, &mut written).await?;
        }
        let staged = futures::future::try_join_all(chain.iter().map(|cv| repo.stage_version(cv)));
        let (staged, ()) = futures::try_join!(
            async { staged.await.map_err(crate::error::Error::from) },
            trees.flush()
        )?;
        let mut tx = rill_store::Transaction::new();
        for (cv, (staged, hash)) in chain.iter().zip(staged.into_iter().zip(&hashes)) {
            tx = tx.merge(staged);
            tx.mutations.extend(repo.event_by(
                &cv.author,
                EventKind::VersionStored {
                    version: *hash,
                    change: cv.change_id,
                },
            )?);
        }
        tx.mutations.extend(written.mutations.iter().cloned());
        match repo.meta().transact(tx).await {
            Ok(_) => self.committed(written),
            Err(e) if e.is_conflict() => {
                for cv in chain {
                    self.store_version(cv).await?;
                }
            }
            Err(e) => return Err(e.into()),
        }
        Ok(hashes)
    }

    async fn row<D: Derivable>(
        &self,
        hash: &ChangeVersionHash,
        value: &D::Value,
        out: &mut LandCritical,
    ) -> Result<()> {
        let kind = derived_kind_key::<D>();
        let bytes = encode_tagged(&kind, value)?;
        out.mutations.push(
            self.repo()
                .derived_mutation(&kind, hash, bytes.clone())
                .await?,
        );
        out.values.push((kind, *hash, bytes));
        Ok(())
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
