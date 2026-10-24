//! The [`SearchIndex`]: segments per shard, path layers per root, and the
//! snapshot queries run against.

use std::collections::HashSet;
use std::sync::Arc;

use parking_lot::RwLock;
use rill_core::{DeriveManager, TreeOps};
use rill_store::RepoStore;
use rill_types::{ObjectHash, TreeHash};

use super::cache::SegmentCache;
use super::config::IndexConfig;
use super::records::{self, SegmentSet, Through};
use super::views::ViewCache;
use crate::error::Result;
use crate::paths::PathView;
use crate::segment::Segment;

/// What queries see: every shard's open segments, at one generation.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub generation: u64,
    pub shards: Vec<Vec<Arc<Segment>>>,
    pub through: Option<Through>,
}

impl Snapshot {
    /// Blobs indexed across every shard.
    pub fn docs(&self) -> usize {
        self.shards.iter().flatten().map(|s| s.docs().len()).sum()
    }

    /// Segments across every shard.
    pub fn segments(&self) -> usize {
        self.shards.iter().map(Vec::len).sum()
    }
}

/// A repository's search index. One process writes it (holding the
/// repository's search lease); every process reads it, reloading when the
/// stored segment list's generation moves.
pub struct SearchIndex {
    pub(super) repo: RepoStore,
    pub(super) derive: Arc<DeriveManager>,
    pub(super) config: IndexConfig,
    pub(super) segments: SegmentCache,
    pub(super) views: ViewCache,
    pub(super) snapshot: RwLock<Arc<Snapshot>>,
    /// The segment list as last stored or loaded; the writer edits a copy.
    pub(super) set: parking_lot::Mutex<SegmentSet>,
    /// Serialises writes and refreshes.
    pub(super) writing: tokio::sync::Mutex<()>,
}

impl SearchIndex {
    /// An empty index over `repo`, not yet loaded.
    pub fn new(repo: RepoStore, derive: Arc<DeriveManager>, config: IndexConfig) -> Arc<Self> {
        Arc::new(SearchIndex {
            segments: SegmentCache::new(config.cache_dir.clone()),
            repo,
            derive,
            config,
            views: ViewCache::default(),
            snapshot: RwLock::new(Arc::default()),
            set: parking_lot::Mutex::new(SegmentSet::default()),
            writing: tokio::sync::Mutex::new(()),
        })
    }

    /// An index loaded from what was stored.
    pub async fn open(
        repo: RepoStore,
        derive: Arc<DeriveManager>,
        config: IndexConfig,
    ) -> Result<Arc<Self>> {
        let index = Self::new(repo, derive, config);
        index.load().await?;
        Ok(index)
    }

    pub fn repo(&self) -> &RepoStore {
        &self.repo
    }

    pub fn trees(&self) -> &TreeOps {
        self.derive.trees()
    }

    pub fn config(&self) -> &IndexConfig {
        &self.config
    }

    /// The current snapshot.
    pub fn snapshot(&self) -> Arc<Snapshot> {
        self.snapshot.read().clone()
    }

    /// How far trunk is indexed, as last loaded.
    pub fn through(&self) -> Option<Through> {
        self.snapshot().through
    }

    /// The shard a blob belongs to.
    pub fn shard_of(&self, hash: &ObjectHash) -> usize {
        let b = hash.as_bytes();
        (u16::from_le_bytes([b[0], b[1]]) % self.config.shards.max(1)) as usize
    }

    /// Load the stored segment list and open its segments.
    pub async fn load(&self) -> Result<()> {
        let _guard = self.writing.lock().await;
        let stored = records::segment_set(&self.repo).await?;
        self.install(stored).await
    }

    /// Reload when another process changed the index; one read otherwise.
    pub async fn refresh(&self) -> Result<()> {
        let stored = records::segment_set(&self.repo).await?;
        if stored.generation == self.snapshot().generation {
            return Ok(());
        }
        let _guard = self.writing.lock().await;
        if stored.generation == self.snapshot().generation {
            return Ok(());
        }
        self.install(stored).await
    }

    /// Open `set`'s segments and make it the snapshot.
    pub(super) async fn install(&self, set: SegmentSet) -> Result<()> {
        let mut shards = Vec::with_capacity(set.shards.len());
        let mut live = HashSet::new();
        for refs in &set.shards {
            let mut open = Vec::with_capacity(refs.len());
            for r in refs {
                open.push(self.segments.get(&self.repo, r).await?);
                live.insert(r.hash);
            }
            shards.push(open);
        }
        self.segments.retain(&live);
        *self.snapshot.write() = Arc::new(Snapshot {
            generation: set.generation,
            shards,
            through: set.through,
        });
        *self.set.lock() = set;
        Ok(())
    }

    /// The files of `root`, when it is indexed.
    pub async fn view(&self, root: TreeHash) -> Result<Option<Arc<PathView>>> {
        self.views.get(&self.repo, root).await
    }

    /// Whether `root` is indexed.
    pub async fn has_root(&self, root: TreeHash) -> Result<bool> {
        Ok(self.view(root).await?.is_some())
    }
}
