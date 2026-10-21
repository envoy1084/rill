//! The [`SearchIndex`]: in-memory postings over a repository, persisted as
//! derived data.

use std::collections::{BTreeSet, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use dashmap::DashMap;
use futures::stream::{self, StreamExt, TryStreamExt};
use parking_lot::{Mutex, RwLock};
use rill_core::{is_binary, read_content, DeriveManager, RootTree, TreeOps};
use rill_store::repo::Lease;
use rill_store::RepoStore;
use rill_types::{ChangeVersionHash, ObjectHash, RepoPathBuf, SparseProfile, TreeHash};

use super::config::IndexConfig;
use super::docs::DocTable;
use super::manifest::{ManifestEntry, RootManifest};
use super::persist::{self, shard_of, BlobMarker, IndexState, KIND_ROOT};
use crate::error::{Error, Result};
use crate::trigram::{trigrams, DocId, PostingList};

/// Outcome of indexing a root.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct IndexStats {
    /// Files in the root.
    pub files: usize,
    /// Blobs tokenized by this call.
    pub blobs_indexed: usize,
    /// Blobs recorded as skipped (binary or over the size limit) by this call.
    pub blobs_skipped: usize,
    /// True when the root already had a manifest and nothing was done.
    pub already_indexed: bool,
}

/// Outcome of a flush.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FlushStats {
    /// Shards rewritten.
    pub shards: usize,
    /// Blob markers written.
    pub markers: usize,
}

/// Trigram index over a repository's blobs, keyed by content hash so that
/// a blob is tokenized once no matter how many roots or paths contain it.
///
/// Postings are always ASCII-case-folded. One folded index serves both
/// case-sensitive and case-insensitive queries: a case-sensitive query folds
/// its literals too, which yields a superset of candidates that the regex
/// then filters exactly. The cost is slightly weaker pruning for
/// case-sensitive searches.
pub struct SearchIndex {
    pub(super) repo: RepoStore,
    pub(super) derive: Arc<DeriveManager>,
    pub(super) config: IndexConfig,
    pub(super) docs: RwLock<DocTable>,
    pub(super) postings: DashMap<u32, PostingList>,
    /// Blobs whose state is known in this process: indexed under a document
    /// id, or skipped.
    pub(super) seen: DashMap<ObjectHash, Option<DocId>>,
    /// Markers to persist at the next flush. Markers are written only after
    /// the postings they refer to, so a crash between the two never leaves a
    /// blob marked indexed without postings.
    pub(super) pending_markers: Mutex<Vec<(ObjectHash, Option<DocId>)>>,
    pub(super) dirty_shards: Mutex<BTreeSet<u8>>,
    pub(super) manifests: DashMap<TreeHash, Arc<RootManifest>>,
    pub(super) blob_index_calls: AtomicU64,
    /// The generations this process last loaded or wrote.
    pub(super) state: Mutex<IndexState>,
    /// Serialises refreshes and flushes.
    pub(super) syncing: tokio::sync::Mutex<()>,
}

impl SearchIndex {
    /// An empty index over `repo`, not yet loaded from the store.
    pub fn new(repo: RepoStore, derive: Arc<DeriveManager>, config: IndexConfig) -> Arc<Self> {
        Arc::new(SearchIndex {
            repo,
            derive,
            config,
            docs: RwLock::new(DocTable::new()),
            postings: DashMap::new(),
            seen: DashMap::new(),
            pending_markers: Mutex::new(Vec::new()),
            dirty_shards: Mutex::new(BTreeSet::new()),
            manifests: DashMap::new(),
            blob_index_calls: AtomicU64::new(0),
            state: Mutex::new(IndexState::default()),
            syncing: tokio::sync::Mutex::new(()),
        })
    }

    /// Create an index and load whatever an earlier flush persisted.
    pub async fn open(
        repo: RepoStore,
        derive: Arc<DeriveManager>,
        config: IndexConfig,
    ) -> Result<Arc<Self>> {
        let index = Self::new(repo, derive, config);
        index.load().await?;
        Ok(index)
    }

    /// The repository this index covers.
    pub fn repo(&self) -> &RepoStore {
        &self.repo
    }

    /// Tree operations shared with the derive manager.
    pub fn trees(&self) -> &TreeOps {
        self.derive.trees()
    }

    /// Number of times a blob was handed to the tokenizer (including ones
    /// that were skipped). Useful for asserting that re-indexing is free.
    pub fn blob_index_calls(&self) -> u64 {
        self.blob_index_calls.load(Ordering::Relaxed)
    }

    /// Number of indexed documents.
    pub fn doc_count(&self) -> usize {
        self.docs.read().len()
    }

    /// Document id of a content hash, if it was indexed (not skipped).
    pub fn doc_of(&self, hash: &ObjectHash) -> Option<DocId> {
        self.docs.read().id_of(hash)
    }

    /// Sorted documents containing `trigram`, or `None` if it never occurs.
    pub fn postings_of(&self, trigram: u32) -> Option<Vec<DocId>> {
        self.postings.get(&trigram).map(|l| l.docs().to_vec())
    }

    /// Tokenize one blob and add it to the postings. Returns its document
    /// id, or `None` when it was skipped. Calling this for a blob that is
    /// already indexed is a no-op returning the existing id.
    pub async fn index_blob(&self, hash: ObjectHash, bytes: Bytes) -> Result<Option<DocId>> {
        if let Some(known) = self.seen.get(&hash) {
            return Ok(*known);
        }
        self.blob_index_calls.fetch_add(1, Ordering::Relaxed);
        if bytes.len() > self.config.max_blob_size || is_binary(&bytes) {
            tracing::debug!(hash = %hash, len = bytes.len(), "skipping blob");
            self.record(hash, None);
            return Ok(None);
        }
        let tris = tokio::task::spawn_blocking(move || trigrams(&bytes, true)).await?;
        let doc = self.docs.write().insert(hash);
        {
            let mut dirty = self.dirty_shards.lock();
            for t in &tris {
                dirty.insert(shard_of(*t));
            }
        }
        for t in tris {
            self.postings.entry(t).or_default().insert(doc);
        }
        self.record(hash, Some(doc));
        Ok(Some(doc))
    }

    fn record(&self, hash: ObjectHash, doc: Option<DocId>) {
        self.seen.insert(hash, doc);
        self.pending_markers.lock().push((hash, doc));
    }

    /// Whether a blob is known to the index, consulting persisted markers
    /// for blobs this process has not touched.
    async fn known(&self, hash: &ObjectHash) -> Result<Option<Option<DocId>>> {
        if let Some(known) = self.seen.get(hash) {
            return Ok(Some(*known));
        }
        match persist::get_marker(&self.repo, hash).await? {
            Some(BlobMarker { doc }) => {
                // A persisted marker whose document is missing from the
                // loaded table means the table was not flushed with it;
                // treat the blob as unknown so it gets re-indexed.
                if doc.is_some_and(|d| self.docs.read().hash_of(d) != Some(*hash)) {
                    return Ok(None);
                }
                self.seen.insert(*hash, doc);
                Ok(Some(doc))
            }
            None => Ok(None),
        }
    }

    /// Index every file under `root`, tokenizing only blobs not yet seen,
    /// flush, and then store the root's manifest, so a manifest is only
    /// ever visible with the postings behind it. A root that already has a
    /// manifest is left alone. Writes are fenced on `fence` when given.
    pub async fn index_root(&self, root: TreeHash, fence: Option<&Lease>) -> Result<IndexStats> {
        if let Some(manifest) = self.load_manifest(root).await? {
            return Ok(IndexStats {
                files: manifest.len(),
                already_indexed: true,
                ..Default::default()
            });
        }
        let entries = self
            .trees()
            .walk(root, &RepoPathBuf::root(), &SparseProfile::full())
            .await?;
        let manifest = RootManifest {
            entries: entries
                .into_iter()
                .map(|e| ManifestEntry {
                    path: e.path,
                    content: e.content,
                    kind: e.kind,
                })
                .collect(),
        };

        let mut todo = Vec::new();
        let mut unique = HashSet::new();
        for entry in &manifest.entries {
            let hash = entry.content_hash();
            if unique.insert(hash) && self.known(&hash).await?.is_none() {
                todo.push(entry.content.clone());
            }
        }

        let objects = self.repo.objects().clone();
        let results: Vec<Option<DocId>> = stream::iter(todo)
            .map(|content| {
                let objects = objects.clone();
                async move {
                    // A redacted file is indexed as empty: it matches nothing.
                    let bytes = match read_content(objects.as_ref(), &content).await {
                        Err(e) if e.is_redacted() => bytes::Bytes::new(),
                        other => other?,
                    };
                    self.index_blob(content.content_hash(), bytes).await
                }
            })
            .buffer_unordered(self.config.concurrency.max(1))
            .try_collect()
            .await?;

        let stats = IndexStats {
            files: manifest.len(),
            blobs_indexed: results.iter().filter(|r| r.is_some()).count(),
            blobs_skipped: results.iter().filter(|r| r.is_none()).count(),
            already_indexed: false,
        };
        self.flush(fence).await?;
        persist::put(
            &self.repo,
            KIND_ROOT,
            &root,
            persist::encode(KIND_ROOT, &manifest)?,
            fence,
        )
        .await?;
        self.manifests.insert(root, Arc::new(manifest));
        tracing::info!(root = %root, ?stats, "indexed root");
        Ok(stats)
    }

    /// Index the root tree of a change version.
    pub async fn index_version(
        &self,
        version: &ChangeVersionHash,
        fence: Option<&Lease>,
    ) -> Result<(TreeHash, IndexStats)> {
        let root = self.derive.get::<RootTree>(version).await?;
        let stats = self.index_root(root, fence).await?;
        Ok((root, stats))
    }

    /// The manifest of an indexed root.
    pub async fn manifest(&self, root: TreeHash) -> Result<Arc<RootManifest>> {
        self.load_manifest(root)
            .await?
            .ok_or(Error::RootNotIndexed(root))
    }

    async fn load_manifest(&self, root: TreeHash) -> Result<Option<Arc<RootManifest>>> {
        if let Some(m) = self.manifests.get(&root) {
            return Ok(Some(m.clone()));
        }
        match self.repo.get_derived(KIND_ROOT, &root).await? {
            Some(bytes) => {
                let manifest: RootManifest = persist::decode(KIND_ROOT, &bytes)?;
                let manifest = Arc::new(manifest);
                self.manifests.insert(root, manifest.clone());
                Ok(Some(manifest))
            }
            None => Ok(None),
        }
    }
}
