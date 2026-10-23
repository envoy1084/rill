//! The view's operations as async functions, one per filesystem call the
//! view serves; `fs.rs` bridges the kernel's calls onto them.

use std::os::unix::fs::FileExt;

use bytes::Bytes;
use fuser::FileAttr;
use rill_types::{FileKind, RepoPath, RepoPathBuf};

use super::attrs::{dir_attr, file_attr};
use super::handles::{read_at, slice, Handle, Scratch};
use super::node::{Node, Source};
use super::view::View;
use crate::error::{Error, Result};
use crate::overlay::OverlayOp;

/// An error with a POSIX code the kernel passes on as is.
pub(super) fn posix(code: i32) -> Error {
    Error::Io(std::io::Error::from_raw_os_error(code))
}

/// What a successful open hands the kernel.
pub struct Opened {
    pub fh: u64,
    /// A cached object the kernel may read directly (passthrough).
    pub backing: Option<std::fs::File>,
}

impl View {
    fn attr_of(&self, ino: u64, node: &Node) -> FileAttr {
        match node {
            Node::Dir => dir_attr(ino, self.owner),
            Node::File { size, kind, source } => {
                file_attr(ino, *size, *kind, &source.hash(), self.owner)
            }
        }
    }

    async fn node(&self, path: &RepoPath) -> Result<Node> {
        self.resolve(path)
            .await?
            .ok_or_else(|| Error::NotFound(path.to_string()))
    }

    pub(super) async fn lookup(&self, parent: u64, name: &std::ffi::OsStr) -> Result<FileAttr> {
        let path = self.child(parent, name)?;
        let node = self.node(&path).await?;
        Ok(self.attr_of(self.ino(&path), &node))
    }

    pub(super) async fn getattr(&self, ino: u64, fh: Option<u64>) -> Result<FileAttr> {
        if let Some(fh) = fh {
            if let Some(Handle::Write(s)) = self.handles.lock().get(&fh) {
                let size = s.file.metadata()?.len();
                let hash = crate::cache::local_digest(s.location.as_os_str().as_encoded_bytes());
                return Ok(file_attr(ino, size, s.kind, &hash, self.owner));
            }
        }
        let path = self.path(ino)?;
        let node = self.node(&path).await?;
        Ok(self.attr_of(ino, &node))
    }

    /// The bytes of a file node.
    async fn bytes(&self, source: &Source) -> Result<Bytes> {
        let content = &self.mount.shared.content;
        match source {
            Source::Base(c) => content.get(&self.mount.repo, c).await,
            Source::Local(d) => content.local(*d).await,
            Source::Marker(b) => Ok(b.clone()),
        }
    }

    pub(super) async fn open(&self, ino: u64, write: bool, truncate: bool) -> Result<Opened> {
        let path = self.path(ino)?;
        let Node::File { kind, source, .. } = self.node(&path).await? else {
            return Err(posix(libc::EISDIR));
        };
        if write {
            if matches!(source, Source::Marker(_)) {
                return Err(posix(libc::EROFS));
            }
            let initial = if truncate {
                Bytes::new()
            } else {
                self.bytes(&source).await?
            };
            let scratch = self.scratch(path, kind, &initial, truncate)?;
            return Ok(Opened {
                fh: self.add_handle(Handle::Write(scratch)),
                backing: None,
            });
        }
        let handle = match &source {
            Source::Marker(b) => Handle::ReadBytes(b.clone()),
            Source::Base(rill_types::ContentRef::Inline { bytes }) => {
                Handle::ReadBytes(bytes.clone())
            }
            Source::Base(c) => {
                self.mount.shared.content.get(&self.mount.repo, c).await?;
                Handle::ReadFile(self.cached(&c.content_hash())?)
            }
            Source::Local(d) => match self.cached(d) {
                Ok(file) => Handle::ReadFile(file),
                Err(_) => Handle::ReadBytes(self.bytes(&source).await?),
            },
        };
        let backing = match &handle {
            Handle::ReadFile(f) => f.try_clone().ok(),
            _ => None,
        };
        Ok(Opened {
            fh: self.add_handle(handle),
            backing,
        })
    }

    fn cached(&self, hash: &rill_types::ObjectHash) -> Result<std::fs::File> {
        Ok(std::fs::File::open(
            self.mount.shared.content.cache().path_of(hash),
        )?)
    }

    fn scratch(
        &self,
        path: RepoPathBuf,
        kind: FileKind,
        initial: &[u8],
        dirty: bool,
    ) -> Result<Scratch> {
        let dir = self.mount.shared.config.dir.scratch().join(self.mount.id());
        let mut s = Scratch::create(&dir, path, kind, initial)?;
        s.dirty = dirty;
        Ok(s)
    }

    pub(super) fn read(&self, fh: u64, offset: u64, size: u32) -> Result<Vec<u8>> {
        match self.handles.lock().get(&fh) {
            Some(Handle::ReadFile(f)) => read_at(f, offset, size),
            Some(Handle::ReadBytes(b)) => Ok(slice(b, offset, size)),
            Some(Handle::Write(s)) => read_at(&s.file, offset, size),
            _ => Err(posix(libc::EBADF)),
        }
    }

    pub(super) fn write(&self, fh: u64, offset: u64, data: &[u8]) -> Result<u32> {
        match self.handles.lock().get_mut(&fh) {
            Some(Handle::Write(s)) => {
                s.file.write_all_at(data, offset)?;
                s.dirty = true;
                Ok(data.len() as u32)
            }
            _ => Err(posix(libc::EBADF)),
        }
    }

    /// Turn a dirty scratch file into an overlay entry.
    pub(super) async fn flush(&self, fh: u64) -> Result<()> {
        let pending = match self.handles.lock().get_mut(&fh) {
            Some(Handle::Write(s)) if s.dirty => {
                s.dirty = false;
                Some((s.path.clone(), s.kind, s.contents()?))
            }
            _ => None,
        };
        if let Some((path, kind, bytes)) = pending {
            self.commit(&path, bytes, kind).await?;
        }
        Ok(())
    }

    pub(super) async fn release(&self, fh: u64) -> Result<()> {
        let flushed = self.flush(fh).await;
        self.handles.lock().remove(&fh);
        flushed
    }

    pub(super) async fn create(
        &self,
        parent: u64,
        name: &std::ffi::OsStr,
        mode: u32,
    ) -> Result<(FileAttr, u64)> {
        let path = self.child(parent, name)?;
        let kind = if mode & 0o111 != 0 {
            FileKind::Executable
        } else {
            FileKind::Regular
        };
        self.commit(&path, Bytes::new(), kind).await?;
        let scratch = self.scratch(path.clone(), kind, &[], false)?;
        let fh = self.add_handle(Handle::Write(scratch));
        let node = self.node(&path).await?;
        Ok((self.attr_of(self.ino(&path), &node), fh))
    }

    pub(super) async fn mkdir(&self, parent: u64, name: &std::ffi::OsStr) -> Result<FileAttr> {
        let path = self.child(parent, name)?;
        if self.resolve(&path).await?.is_some() {
            return Err(posix(libc::EEXIST));
        }
        {
            let mut dirs = self.dirs.lock();
            dirs.removed.remove(&path);
            dirs.created.insert(path.clone());
        }
        Ok(dir_attr(self.ino(&path), self.owner))
    }

    pub(super) async fn unlink(&self, parent: u64, name: &std::ffi::OsStr) -> Result<()> {
        let path = self.child(parent, name)?;
        match self.node(&path).await? {
            Node::Dir => Err(posix(libc::EISDIR)),
            Node::File {
                source: Source::Marker(_),
                ..
            } => Err(posix(libc::EROFS)),
            Node::File { .. } => self.record(vec![(path, OverlayOp::Delete)]).await,
        }
    }

    pub(super) async fn rmdir(&self, parent: u64, name: &std::ffi::OsStr) -> Result<()> {
        let path = self.child(parent, name)?;
        match self.list(&path).await? {
            None => Err(posix(libc::ENOTDIR)),
            Some(entries) if !entries.is_empty() => Err(posix(libc::ENOTEMPTY)),
            Some(_) => {
                let mut dirs = self.dirs.lock();
                dirs.created.remove(&path);
                dirs.removed.insert(path);
                Ok(())
            }
        }
    }

    pub(super) async fn symlink(
        &self,
        parent: u64,
        name: &std::ffi::OsStr,
        target: &std::path::Path,
    ) -> Result<FileAttr> {
        let path = self.child(parent, name)?;
        let target = target
            .to_str()
            .ok_or_else(|| Error::Invalid("a non-UTF-8 symlink target".into()))?;
        self.commit(
            &path,
            Bytes::copy_from_slice(target.as_bytes()),
            FileKind::Symlink,
        )
        .await?;
        let node = self.node(&path).await?;
        Ok(self.attr_of(self.ino(&path), &node))
    }

    pub(super) async fn readlink(&self, ino: u64) -> Result<Bytes> {
        let path = self.path(ino)?;
        match self.node(&path).await? {
            Node::File {
                kind: FileKind::Symlink,
                source,
                ..
            } => self.bytes(&source).await,
            _ => Err(posix(libc::EINVAL)),
        }
    }

    /// Move a file, or a directory and everything below it, in one
    /// overlay transaction.
    pub(super) async fn rename(
        &self,
        parent: u64,
        name: &std::ffi::OsStr,
        new_parent: u64,
        new_name: &std::ffi::OsStr,
    ) -> Result<()> {
        let from = self.child(parent, name)?;
        let to = self.child(new_parent, new_name)?;
        let node = self.node(&from).await?;
        let mut ops = Vec::new();
        match node {
            Node::File {
                source: Source::Marker(_),
                ..
            } => return Err(posix(libc::EROFS)),
            Node::File { size, kind, source } => {
                let digest = match source {
                    Source::Local(d) => d,
                    other => {
                        let bytes = self.bytes(&other).await?;
                        self.mount.shared.content.put_local(bytes).await?
                    }
                };
                if let Some(Node::Dir) = self.resolve(&to).await? {
                    return Err(posix(libc::EISDIR));
                }
                ops.push((to.clone(), OverlayOp::Set { digest, kind, size }));
                ops.push((from.clone(), OverlayOp::Delete));
            }
            Node::Dir => {
                for (path, size, kind, source) in self.files_below(&from).await? {
                    let digest = match source {
                        Source::Local(d) => d,
                        other => {
                            let bytes = self.bytes(&other).await?;
                            self.mount.shared.content.put_local(bytes).await?
                        }
                    };
                    let Some(rest) = path.strip_prefix(&from) else {
                        continue;
                    };
                    let mut target = to.clone();
                    rest.components().try_for_each(|c| target.push(c))?;
                    ops.push((target, OverlayOp::Set { digest, kind, size }));
                    ops.push((path.clone(), OverlayOp::Delete));
                }
                let mut dirs = self.dirs.lock();
                dirs.removed.insert(from.clone());
                dirs.created.insert(to.clone());
            }
        }
        self.record(ops).await?;
        self.inodes.lock().rename(&from, &to);
        Ok(())
    }

    /// Every file at or below directory `dir` as the view shows it.
    async fn files_below(
        &self,
        dir: &RepoPath,
    ) -> Result<Vec<(RepoPathBuf, u64, FileKind, Source)>> {
        let mut out = Vec::new();
        let mut pending = vec![dir.to_owned_path()];
        while let Some(d) = pending.pop() {
            let Some(entries) = self.list(&d).await? else {
                continue;
            };
            for (name, is_dir) in entries {
                let child = d.join(&name)?;
                if is_dir {
                    pending.push(child);
                } else if let Some(Node::File { size, kind, source }) = self.resolve(&child).await?
                {
                    out.push((child, size, kind, source));
                }
            }
        }
        Ok(out)
    }

    /// Truncate or change the mode of a file.
    pub(super) async fn setattr(
        &self,
        ino: u64,
        fh: Option<u64>,
        size: Option<u64>,
        mode: Option<u32>,
    ) -> Result<FileAttr> {
        let path = self.path(ino)?;
        if let (Some(size), Some(fh)) = (size, fh) {
            if let Some(Handle::Write(s)) = self.handles.lock().get_mut(&fh) {
                s.file.set_len(size)?;
                s.dirty = true;
            }
        } else if let Some(size) = size {
            if let Node::File { kind, source, .. } = self.node(&path).await? {
                let mut bytes = self.bytes(&source).await?.to_vec();
                bytes.resize(size as usize, 0);
                self.commit(&path, Bytes::from(bytes), kind).await?;
            }
        }
        if let Some(mode) = mode {
            if let Node::File { kind, source, size } = self.node(&path).await? {
                let wanted = match (kind, mode & 0o111 != 0) {
                    (FileKind::Symlink, _) => FileKind::Symlink,
                    (_, true) => FileKind::Executable,
                    (_, false) => FileKind::Regular,
                };
                if wanted != kind {
                    let digest = match source {
                        Source::Local(d) => d,
                        other => {
                            let bytes = self.bytes(&other).await?;
                            self.mount.shared.content.put_local(bytes).await?
                        }
                    };
                    self.record(vec![(
                        path.clone(),
                        OverlayOp::Set {
                            digest,
                            kind: wanted,
                            size,
                        },
                    )])
                    .await?;
                }
            }
        }
        self.getattr(ino, fh).await
    }

    pub(super) async fn opendir(&self, ino: u64) -> Result<u64> {
        let path = self.path(ino)?;
        let entries = self
            .list(&path)
            .await?
            .ok_or_else(|| posix(libc::ENOTDIR))?;
        let mut listing = Vec::with_capacity(entries.len());
        for (name, is_dir) in entries {
            let child = path.join(&name)?;
            listing.push((self.ino(&child), is_dir, name));
        }
        Ok(self.add_handle(Handle::Dir(listing)))
    }
}
