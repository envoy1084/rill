//! The state of one mounted view: the mount, inode numbers, directories
//! made or removed locally (directories are not versioned), open handles,
//! and the runtime the filesystem callbacks run their async work on.

use std::collections::{BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::Mutex;
use rill_types::{FileKind, RepoPath, RepoPathBuf};

use super::attrs::Owner;
use super::handles::Handle;
use super::inodes::Inodes;
use crate::error::{Error, Result};
use crate::mount::{Change, Mount};
use crate::overlay::OverlayOp;

/// Directories created or removed in the view since it was mounted.
#[derive(Debug, Default)]
pub struct LocalDirs {
    pub created: BTreeSet<RepoPathBuf>,
    pub removed: BTreeSet<RepoPathBuf>,
}

/// A mounted view.
pub struct View {
    pub(super) mount: Arc<Mount>,
    pub(super) rt: tokio::runtime::Handle,
    pub(super) inodes: Mutex<Inodes>,
    pub(super) dirs: Mutex<LocalDirs>,
    pub(super) handles: Mutex<HashMap<u64, Handle>>,
    next_handle: AtomicU64,
    pub(super) owner: Owner,
    /// The marker file the view shows at `.rill/mount.json`.
    pub(super) marker: Bytes,
    /// Cleared after the kernel refuses passthrough once.
    pub(super) passthrough: AtomicBool,
}

impl View {
    /// A view of `mount` showing `marker`.
    pub fn new(mount: Arc<Mount>, rt: tokio::runtime::Handle, marker: Bytes) -> Self {
        View {
            mount,
            rt,
            inodes: Mutex::new(Inodes::default()),
            dirs: Mutex::new(LocalDirs::default()),
            handles: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(1),
            owner: Owner::current(),
            marker,
            passthrough: AtomicBool::new(true),
        }
    }

    /// The path of `ino`.
    pub(super) fn path(&self, ino: u64) -> Result<RepoPathBuf> {
        self.inodes
            .lock()
            .path(ino)
            .ok_or_else(|| Error::NotFound(format!("inode {ino}")))
    }

    /// The child `name` of directory `parent`.
    pub(super) fn child(&self, parent: u64, name: &std::ffi::OsStr) -> Result<RepoPathBuf> {
        let name = name
            .to_str()
            .ok_or_else(|| Error::Invalid("a non-UTF-8 name".into()))?;
        Ok(self.path(parent)?.join(name)?)
    }

    pub(super) fn ino(&self, path: &RepoPath) -> u64 {
        self.inodes.lock().ino(path)
    }

    pub(super) fn add_handle(&self, handle: Handle) -> u64 {
        let fh = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.handles.lock().insert(fh, handle);
        fh
    }

    /// Record `bytes` as the content of `path`, keeping its conflict flag.
    pub(super) async fn commit(&self, path: &RepoPath, bytes: Bytes, kind: FileKind) -> Result<()> {
        let size = bytes.len() as u64;
        let digest = self.mount.shared.content.put_local(bytes).await?;
        let conflict = self.mount.entry(path).is_some_and(|e| e.conflict);
        self.mount
            .write(
                Change {
                    overlay: vec![(
                        path.to_owned_path(),
                        Some((OverlayOp::Set { digest, kind, size }, conflict)),
                    )],
                    ..Change::default()
                },
                None,
            )
            .await
    }

    /// Record a batch of overlay operations in one transaction.
    pub(super) async fn record(&self, ops: Vec<(RepoPathBuf, OverlayOp)>) -> Result<()> {
        let overlay = ops
            .into_iter()
            .map(|(p, op)| {
                let conflict = self.mount.entry(&p).is_some_and(|e| e.conflict);
                (p, Some((op, conflict)))
            })
            .collect();
        self.mount
            .write(
                Change {
                    overlay,
                    ..Change::default()
                },
                None,
            )
            .await
    }
}
