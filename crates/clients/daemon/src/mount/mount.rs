//! A mount at run time: its record, overlay and base in memory, kept in
//! step with the state database. Every write is persisted before it is
//! visible, under one writer lock so sequence numbers stay ordered.

use std::sync::{Arc, OnceLock};

use parking_lot::{Mutex, RwLock};
use rill_sdk::RepoClient;
use rill_types::{ContentRef, ObjectHash, RepoPath, RepoPathBuf, TreeHash};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::record::{MountMode, MountRecord};
use crate::eager::Watch;
use crate::error::Result;
use crate::overlay::{BaseFile, Overlay, OverlayEntry, OverlayOp};
use crate::shared::Shared;
use crate::state::{Batch, FileStat};
use crate::view::Base;

/// A set of changes to apply to a mount atomically.
#[derive(Debug, Default)]
pub struct Change {
    /// Overlay operations (`None` forgets the path's entry), with the
    /// conflict flag.
    pub overlay: Vec<(RepoPathBuf, Option<(OverlayOp, bool)>)>,
    /// Stat cache updates of an eager mount.
    pub files: Vec<(RepoPathBuf, Option<FileStat>)>,
    /// A new record (base, working change, sync position).
    pub record: Option<MountRecord>,
}

impl Change {
    /// True when nothing would change.
    pub fn is_empty(&self) -> bool {
        self.overlay.is_empty() && self.files.is_empty() && self.record.is_none()
    }
}

/// A served mount.
pub struct Mount {
    pub(crate) shared: Arc<Shared>,
    pub(crate) repo: RepoClient,
    record: RwLock<MountRecord>,
    overlay: RwLock<Overlay>,
    base: RwLock<Base>,
    writes: tokio::sync::Mutex<()>,
    /// Serializes push, commit, update and resolve.
    pub(crate) ops: tokio::sync::Mutex<()>,
    pub(crate) dirty: Notify,
    pub(crate) sync_error: Mutex<Option<String>>,
    pub(crate) cancel: CancellationToken,
    pub(crate) watch: OnceLock<Arc<Watch>>,
    /// Content already on the server, by local digest.
    pub(crate) uploaded: moka::sync::Cache<ObjectHash, ContentRef>,
    /// Keeps a lazy mount's view mounted.
    pub(crate) view: Mutex<Option<Box<dyn Send>>>,
    /// Commits what a view still buffers (macOS writes between flushes).
    pub(crate) settler: OnceLock<Settler>,
}

/// Commits a view's buffered writes.
pub type Settler = Arc<dyn Fn() -> futures::future::BoxFuture<'static, Result<()>> + Send + Sync>;

impl std::fmt::Debug for Mount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mount")
            .field("record", &*self.record.read())
            .finish_non_exhaustive()
    }
}

impl Mount {
    /// A mount over stored state.
    pub(crate) fn new(
        shared: Arc<Shared>,
        repo: RepoClient,
        record: MountRecord,
        entries: Vec<(RepoPathBuf, OverlayEntry)>,
    ) -> Self {
        for (_, entry) in &entries {
            if let Some(d) = entry.digest() {
                if !shared.content.cache().pin(&d) {
                    tracing::warn!(digest = %d, "overlay content missing from the cache");
                }
            }
        }
        let base = Base::new(repo.clone(), record.base_tree, shared.trees.clone());
        let overlay = Overlay::restore(entries, record.synced);
        Mount {
            shared,
            repo,
            record: RwLock::new(record),
            overlay: RwLock::new(overlay),
            base: RwLock::new(base),
            writes: tokio::sync::Mutex::new(()),
            ops: tokio::sync::Mutex::new(()),
            dirty: Notify::new(),
            sync_error: Mutex::new(None),
            cancel: CancellationToken::new(),
            watch: OnceLock::new(),
            uploaded: moka::sync::Cache::new(1 << 16),
            view: Mutex::new(None),
            settler: OnceLock::new(),
        }
    }

    /// The mount's id.
    pub fn id(&self) -> String {
        self.record.read().id.clone()
    }

    /// A copy of the record.
    pub fn record(&self) -> MountRecord {
        self.record.read().clone()
    }

    /// How files are presented.
    pub fn mode(&self) -> MountMode {
        self.record.read().mode
    }

    /// The base tree.
    pub fn base(&self) -> Base {
        self.base.read().clone()
    }

    /// The overlay entry for `path`.
    pub fn entry(&self, path: &RepoPath) -> Option<OverlayEntry> {
        self.overlay.read().get(path).copied()
    }

    /// Overlay entries at or below `dir`.
    pub fn entries_under(&self, dir: &RepoPath) -> Vec<(RepoPathBuf, OverlayEntry)> {
        self.overlay
            .read()
            .under(dir)
            .map(|(p, e)| (p.clone(), *e))
            .collect()
    }

    /// Every overlay entry.
    pub fn entries(&self) -> Vec<(RepoPathBuf, OverlayEntry)> {
        self.overlay
            .read()
            .iter()
            .map(|(p, e)| (p.clone(), *e))
            .collect()
    }

    /// Entries not yet pushed, and the sequence they reach.
    pub fn pending(&self) -> (Vec<(RepoPathBuf, OverlayEntry)>, u64) {
        let overlay = self.overlay.read();
        let entries = overlay.pending().map(|(p, e)| (p.clone(), *e)).collect();
        (entries, overlay.last_seq())
    }

    /// Count of entries not yet pushed.
    pub fn pending_count(&self) -> usize {
        self.overlay.read().pending_count()
    }

    /// The base file at `path` as status compares it.
    pub async fn base_file(&self, path: &RepoPath) -> Result<Option<BaseFile>> {
        self.base_file_in(&self.base(), path).await
    }

    pub(crate) async fn base_file_in(
        &self,
        base: &Base,
        path: &RepoPath,
    ) -> Result<Option<BaseFile>> {
        Ok(match base.file(path).await? {
            Some((content, kind)) => Some(BaseFile {
                digest: self.shared.content.digest(&self.repo, &content).await?,
                kind,
            }),
            None => None,
        })
    }

    /// The file at `path` as the working copy has it: the overlay's
    /// state, else the base's.
    pub async fn current(&self, path: &RepoPath) -> Result<Option<BaseFile>> {
        match self.entry(path) {
            Some(OverlayEntry {
                op: OverlayOp::Set { digest, kind, .. },
                ..
            }) => Ok(Some(BaseFile { digest, kind })),
            Some(_) => Ok(None),
            None => self.base_file(path).await,
        }
    }

    /// Persist and apply `change`. `base_tree` swaps the base in the same
    /// step (the record must carry it too).
    pub async fn write(&self, change: Change, base_tree: Option<TreeHash>) -> Result<()> {
        if change.is_empty() {
            return Ok(());
        }
        let _writer = self.writes.lock().await;
        let mut entries = Vec::with_capacity(change.overlay.len());
        {
            let mut overlay = self.overlay.write();
            for (path, op) in change.overlay {
                let entry = op.map(|(op, conflict)| OverlayEntry {
                    op,
                    seq: overlay.reserve(),
                    conflict,
                });
                entries.push((path, entry));
            }
        }
        let batch = Batch {
            mount: change.record.clone(),
            overlay: entries,
            files: change.files,
        };
        let id = self.id();
        let batch = {
            let db = self.shared.db.clone();
            tokio::task::spawn_blocking(move || db.apply(&id, &batch).map(|()| batch)).await??
        };
        let touched = !batch.overlay.is_empty();
        let cache = self.shared.content.cache();
        {
            let mut overlay = self.overlay.write();
            for (path, entry) in batch.overlay {
                if let Some(d) = entry.and_then(|e| e.digest()) {
                    cache.pin(&d);
                }
                let old = match entry {
                    Some(e) => overlay.apply(path, e),
                    None => overlay.remove(&path),
                };
                if let Some(d) = old.and_then(|e| e.digest()) {
                    cache.unpin(&d);
                }
            }
            if let Some(record) = &change.record {
                overlay.mark_unsynced();
                overlay.mark_synced(record.synced);
            }
        }
        if let Some(record) = change.record {
            *self.record.write() = record;
        }
        if let Some(tree) = base_tree {
            let base = self.base.read().at(tree);
            *self.base.write() = base;
        }
        if touched && self.mode() != MountMode::Snapshot {
            self.dirty.notify_one();
        }
        Ok(())
    }
}
