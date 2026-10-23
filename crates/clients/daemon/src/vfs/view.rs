//! The state of one mounted view: the mount, inode numbers, directories
//! made or removed locally (directories are not versioned), open handles,
//! and the runtime the filesystem callbacks run their async work on.

use std::collections::{BTreeMap, BTreeSet, HashMap};
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
    pub(crate) mount: Arc<Mount>,
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) rt: tokio::runtime::Handle,
    pub(crate) inodes: Mutex<Inodes>,
    pub(crate) dirs: Mutex<LocalDirs>,
    pub(crate) handles: Mutex<HashMap<u64, Handle>>,
    next_handle: AtomicU64,
    pub(crate) owner: Owner,
    /// Read-only files the view synthesizes (the checkout marker).
    pub(crate) virtuals: BTreeMap<RepoPathBuf, Bytes>,
    /// Cleared after the kernel refuses passthrough once (FUSE).
    #[cfg_attr(not(target_os = "linux"), allow(dead_code))]
    pub(crate) passthrough: AtomicBool,
    /// Cached objects kept open for reads without a handle.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub(crate) files: moka::sync::Cache<rill_types::ObjectHash, Arc<std::fs::File>>,
}

impl View {
    /// A view of `mount` that also shows `virtuals`.
    pub fn new(
        mount: Arc<Mount>,
        rt: tokio::runtime::Handle,
        virtuals: BTreeMap<RepoPathBuf, Bytes>,
    ) -> Self {
        View {
            mount,
            rt,
            inodes: Mutex::new(Inodes::default()),
            dirs: Mutex::new(LocalDirs::default()),
            handles: Mutex::new(HashMap::new()),
            next_handle: AtomicU64::new(1),
            owner: Owner::current(),
            virtuals,
            passthrough: AtomicBool::new(true),
            files: moka::sync::Cache::new(1024),
        }
    }

    /// The path of `ino`.
    pub(crate) fn path(&self, ino: u64) -> Result<RepoPathBuf> {
        self.inodes
            .lock()
            .path(ino)
            .ok_or_else(|| Error::NotFound(format!("inode {ino}")))
    }

    /// The child `name` of directory `parent`.
    pub(crate) fn child(&self, parent: u64, name: &std::ffi::OsStr) -> Result<RepoPathBuf> {
        let name = name
            .to_str()
            .ok_or_else(|| Error::Invalid("a non-UTF-8 name".into()))?;
        Ok(self.path(parent)?.join(name)?)
    }

    pub(crate) fn ino(&self, path: &RepoPath) -> u64 {
        self.inodes.lock().ino(path)
    }

    pub(crate) fn add_handle(&self, handle: Handle) -> u64 {
        let fh = self.next_handle.fetch_add(1, Ordering::Relaxed);
        self.handles.lock().insert(fh, handle);
        fh
    }

    /// Record `bytes` as the content of `path`, keeping its conflict flag.
    pub(crate) async fn commit(&self, path: &RepoPath, bytes: Bytes, kind: FileKind) -> Result<()> {
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
    pub(crate) async fn record(&self, ops: Vec<(RepoPathBuf, OverlayOp)>) -> Result<()> {
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
