//! Fetch-or-compute for derived values.

use std::collections::HashSet;
use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use rill_store::repo::{decode_tagged, encode_tagged};
use rill_store::RepoStore;
use rill_types::{ChangeVersion, ChangeVersionHash, ObjectHash};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::Mutex;

use super::ctx::DeriveCtx;
use super::traits::{derived_kind_key, Derivable};
use crate::error::{Error, Result};
use crate::tree::TreeOps;
use moka::sync::Cache;

type Key = (String, ObjectHash);

/// Computes, stores and caches derived values for one repository.
pub struct DeriveManager {
    repo: RepoStore,
    trees: TreeOps,
    cache: Cache<Key, Bytes>,
    /// One lock per key being derived, so concurrent requests for the same
    /// value compute it once and the rest read it from the store.
    in_flight: DashMap<Key, Arc<Mutex<()>>>,
    /// Resolves text conflicts the line merge leaves, when configured.
    merger: std::sync::OnceLock<Arc<dyn crate::merge::ContentMerger>>,
}

impl DeriveManager {
    /// Create a manager with a cache of `cache_size` encoded values.
    pub fn new(repo: RepoStore, trees: TreeOps, cache_size: usize) -> Arc<Self> {
        Arc::new(DeriveManager {
            repo,
            trees,
            cache: Cache::new(cache_size as u64),
            in_flight: DashMap::new(),
            merger: std::sync::OnceLock::new(),
        })
    }

    /// Use `merger` for text conflicts in rebases and merges; set once.
    pub fn set_merger(&self, merger: Arc<dyn crate::merge::ContentMerger>) {
        if self.merger.set(merger).is_err() {
            tracing::warn!("content merger set twice; keeping the first");
        }
    }

    /// The configured content merger, if any.
    pub fn merger(&self) -> Option<&Arc<dyn crate::merge::ContentMerger>> {
        self.merger.get()
    }

    /// The repository this manager derives for.
    pub fn repo(&self) -> &RepoStore {
        &self.repo
    }

    /// Tree operations shared with derivations.
    pub fn trees(&self) -> &TreeOps {
        &self.trees
    }

    /// Build a derivation context.
    pub fn ctx(self: &Arc<Self>) -> DeriveCtx {
        DeriveCtx::new(self.repo.clone(), self.trees.clone(), self.clone())
    }

    /// The value of `D` for `hash`, computing and storing it if needed.
    pub async fn get<D: Derivable>(self: &Arc<Self>, hash: &ChangeVersionHash) -> Result<D::Value> {
        let kind = derived_kind_key::<D>();
        if let Some(value) = self.load::<D>(&kind, hash).await? {
            return Ok(value);
        }
        let lock = self
            .in_flight
            .entry((kind.clone(), *hash))
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone();
        let _guard = lock.lock().await;
        let result = match self.load::<D>(&kind, hash).await? {
            Some(value) => Ok(value),
            None => self.compute::<D>(&kind, hash).await,
        };
        drop(_guard);
        self.in_flight.remove(&(kind, *hash));
        result
    }

    /// Compute every value landing needs for `hash`: its root tree,
    /// changed paths and ancestry. Storing a version calls this before
    /// acknowledging it, and the lander again before it moves trunk, so
    /// every trunk version has them.
    pub async fn ensure_land_critical(self: &Arc<Self>, hash: &ChangeVersionHash) -> Result<()> {
        for kind in super::registry::scheduled(super::Schedule::LandCritical) {
            kind.compute(self, *hash).await?;
        }
        Ok(())
    }

    /// Compute every background kind of `hash`, ahead of reads.
    pub async fn ensure_background(self: &Arc<Self>, hash: &ChangeVersionHash) -> Result<()> {
        for kind in super::registry::scheduled(super::Schedule::Background) {
            kind.compute(self, *hash).await?;
        }
        Ok(())
    }

    /// Whether every land-critical value of `hash` is stored.
    pub async fn has_land_critical(self: &Arc<Self>, hash: &ChangeVersionHash) -> Result<bool> {
        for kind in super::registry::scheduled(super::Schedule::LandCritical) {
            if !kind.is_stored(self, *hash).await? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Compute the built-in kind stored as `key` (`kind@version`) for
    /// `hash`. Returns `false` for a kind the registry does not know.
    pub async fn compute_kind(
        self: &Arc<Self>,
        key: &str,
        hash: &ChangeVersionHash,
    ) -> Result<bool> {
        match super::registry::kinds().into_iter().find(|k| k.key == key) {
            Some(kind) => kind.compute(self, *hash).await.map(|_| true),
            None => Ok(false),
        }
    }

    /// Whether the value of `D` for `hash` is stored.
    pub async fn has<D: Derivable>(&self, hash: &ChangeVersionHash) -> Result<bool> {
        Ok(self
            .load_raw::<D::Value>(&derived_kind_key::<D>(), hash)
            .await?
            .is_some())
    }

    /// A stored value under `kind` for a key that is not a change version
    /// (for example a `(version, path)` digest).
    pub async fn get_keyed<V: DeserializeOwned>(
        &self,
        kind: &str,
        key: &ObjectHash,
    ) -> Result<Option<V>> {
        self.load_raw(kind, key).await
    }

    /// Store a value under `kind` and an arbitrary key.
    pub async fn put_keyed<V: Serialize>(
        &self,
        kind: &str,
        key: &ObjectHash,
        value: &V,
    ) -> Result<()> {
        self.store(kind, key, value).await
    }

    async fn load<D: Derivable>(&self, kind: &str, hash: &ObjectHash) -> Result<Option<D::Value>> {
        self.load_raw(kind, hash).await
    }

    async fn load_raw<V: DeserializeOwned>(
        &self,
        kind: &str,
        hash: &ObjectHash,
    ) -> Result<Option<V>> {
        let key = (kind.to_string(), *hash);
        let bytes = match self.cache.get(&key) {
            Some(b) => Some(b),
            None => {
                let fetched = self.repo.get_derived(kind, hash).await?;
                if let Some(b) = &fetched {
                    self.cache.insert(key, b.clone());
                }
                fetched
            }
        };
        match bytes {
            Some(b) => Ok(Some(decode_tagged(kind, &b)?)),
            None => Ok(None),
        }
    }

    async fn store<V: Serialize>(&self, kind: &str, hash: &ObjectHash, value: &V) -> Result<()> {
        let bytes = encode_tagged(kind, value)?;
        self.repo.put_derived(kind, hash, bytes.clone()).await?;
        self.cache.insert((kind.to_string(), *hash), bytes);
        Ok(())
    }

    /// Compute `D` for `hash`, deriving missing ancestors first.
    ///
    /// Uses an explicit stack in post-order: a version is computed once
    /// every parent it needs has a stored value. Histories can be millions
    /// of versions deep, so recursion is not an option.
    async fn compute<D: Derivable>(
        self: &Arc<Self>,
        kind: &str,
        hash: &ChangeVersionHash,
    ) -> Result<D::Value> {
        let ctx = self.ctx();
        let mut stack: Vec<ChangeVersionHash> = vec![*hash];
        let mut expanded: HashSet<ChangeVersionHash> = HashSet::new();
        let mut last: Option<D::Value> = None;
        while let Some(&current) = stack.last() {
            if current != *hash || expanded.contains(&current) {
                if let Some(value) = self.load::<D>(kind, &current).await? {
                    stack.pop();
                    last = Some(value);
                    continue;
                }
            }
            let cv = self.change(&current).await?;
            if D::NEEDS_PARENTS && !expanded.contains(&current) {
                expanded.insert(current);
                let mut missing = Vec::new();
                for parent in &cv.parents {
                    if self.load::<D>(kind, parent).await?.is_none() {
                        missing.push(*parent);
                    }
                }
                if !missing.is_empty() {
                    stack.extend(missing);
                    continue;
                }
            }
            let mut parents = Vec::with_capacity(cv.parents.len());
            if D::NEEDS_PARENTS {
                for parent in &cv.parents {
                    let value =
                        self.load::<D>(kind, parent)
                            .await?
                            .ok_or_else(|| Error::Derive {
                                kind: D::KIND,
                                hash: *parent,
                                reason: "parent value vanished during derivation".into(),
                            })?;
                    parents.push(value);
                }
            }
            tracing::debug!(kind, hash = %current, "deriving");
            let value = D::derive(&ctx, &cv, parents).await?;
            self.store(kind, &current, &value).await?;
            stack.pop();
            last = Some(value);
        }
        last.ok_or_else(|| Error::Derive {
            kind: D::KIND,
            hash: *hash,
            reason: "empty worklist".into(),
        })
    }

    /// Load a change version, mapping absence to [`Error::ChangeNotFound`].
    pub async fn change(&self, hash: &ChangeVersionHash) -> Result<ChangeVersion> {
        self.repo
            .get_change_version(hash)
            .await?
            .ok_or(Error::ChangeNotFound(*hash))
    }
}
