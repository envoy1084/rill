//! The view as an NFSv3 file system for `nfsserve`.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use nfsserve::nfs::{
    fattr3, fileid3, filename3, ftype3, nfspath3, nfsstat3, nfstime3, sattr3, set_mode3, set_size3,
    specdata3,
};
use nfsserve::vfs::{DirEntry, NFSFileSystem, ReadDirResult, VFSCapabilities};

use super::writes::Writes;
use crate::error::Error;
use crate::vfs::{Attr, AttrKind, View};

/// Files macOS writes for its own bookkeeping, refused so they never
/// reach the overlay.
fn is_finder_litter(name: &[u8]) -> bool {
    name == b".DS_Store" || name.starts_with(b"._")
}

/// The NFS side of a view.
pub struct NfsView {
    pub view: Arc<View>,
    pub writes: Arc<Writes>,
    fsid: u64,
}

impl NfsView {
    pub fn new(view: Arc<View>, writes: Arc<Writes>, fsid: u64) -> Self {
        NfsView { view, writes, fsid }
    }

    fn fattr(&self, a: &Attr) -> fattr3 {
        let since = a.mtime.duration_since(UNIX_EPOCH).unwrap_or_default();
        let time = nfstime3 {
            seconds: since.as_secs() as u32,
            nseconds: since.subsec_nanos(),
        };
        fattr3 {
            ftype: match a.kind {
                AttrKind::Dir => ftype3::NF3DIR,
                AttrKind::File => ftype3::NF3REG,
                AttrKind::Symlink => ftype3::NF3LNK,
            },
            mode: a.perm as u32,
            nlink: if a.kind == AttrKind::Dir { 2 } else { 1 },
            uid: a.uid,
            gid: a.gid,
            size: a.size,
            used: a.size,
            rdev: specdata3::default(),
            fsid: self.fsid,
            fileid: a.ino,
            atime: time,
            mtime: time,
            ctime: time,
        }
    }

    async fn attr(&self, ino: u64) -> Result<fattr3, nfsstat3> {
        let fh = self.writes.get(ino);
        let a = self.view.getattr(ino, fh).await.map_err(|e| status(&e))?;
        Ok(self.fattr(&a))
    }

    /// Commit every buffered write before the namespace changes, so a
    /// rename or delete never races a scratch copy.
    async fn settle(&self) -> Result<(), nfsstat3> {
        self.writes
            .commit(&self.view, std::time::Duration::ZERO)
            .await
            .map_err(|e| status(&e))
    }
}

/// A daemon error as an NFS status.
pub fn status(e: &Error) -> nfsstat3 {
    match e {
        Error::NotFound(_) => nfsstat3::NFS3ERR_NOENT,
        Error::Invalid(_) => nfsstat3::NFS3ERR_INVAL,
        Error::Io(io) => match io.raw_os_error() {
            Some(libc::ENOENT) => nfsstat3::NFS3ERR_NOENT,
            Some(libc::EEXIST) => nfsstat3::NFS3ERR_EXIST,
            Some(libc::ENOTEMPTY) => nfsstat3::NFS3ERR_NOTEMPTY,
            Some(libc::EISDIR) => nfsstat3::NFS3ERR_ISDIR,
            Some(libc::ENOTDIR) => nfsstat3::NFS3ERR_NOTDIR,
            Some(libc::EROFS) => nfsstat3::NFS3ERR_ROFS,
            Some(libc::EACCES) => nfsstat3::NFS3ERR_ACCES,
            Some(libc::EINVAL) => nfsstat3::NFS3ERR_INVAL,
            Some(libc::EBADF) => nfsstat3::NFS3ERR_BADHANDLE,
            Some(libc::ENOSPC) => nfsstat3::NFS3ERR_NOSPC,
            _ => nfsstat3::NFS3ERR_IO,
        },
        other => {
            tracing::warn!(error = %other, "view operation failed");
            nfsstat3::NFS3ERR_IO
        }
    }
}

fn name(n: &filename3) -> &OsStr {
    OsStr::from_bytes(n.as_ref())
}

#[async_trait]
impl NFSFileSystem for NfsView {
    fn capabilities(&self) -> VFSCapabilities {
        VFSCapabilities::ReadWrite
    }

    fn root_dir(&self) -> fileid3 {
        crate::vfs::ROOT
    }

    async fn lookup(&self, dirid: fileid3, filename: &filename3) -> Result<fileid3, nfsstat3> {
        match filename.as_ref() {
            b"." => return Ok(dirid),
            b".." => {
                let path = self.view.path(dirid).map_err(|e| status(&e))?;
                return Ok(self.view.ino(path.parent()));
            }
            _ => {}
        }
        let a = self
            .view
            .lookup(dirid, name(filename))
            .await
            .map_err(|e| status(&e))?;
        Ok(a.ino)
    }

    async fn getattr(&self, id: fileid3) -> Result<fattr3, nfsstat3> {
        self.attr(id).await
    }

    async fn setattr(&self, id: fileid3, setattr: sattr3) -> Result<fattr3, nfsstat3> {
        let size = match setattr.size {
            set_size3::size(s) => Some(s),
            set_size3::Void => None,
        };
        let mode = match setattr.mode {
            set_mode3::mode(m) => Some(m),
            set_mode3::Void => None,
        };
        if mode.is_some() {
            self.settle().await?;
        }
        let fh = self.writes.get(id);
        let a = self
            .view
            .setattr(id, fh, size, mode)
            .await
            .map_err(|e| status(&e))?;
        self.writes.touch(id);
        Ok(self.fattr(&a))
    }

    async fn read(
        &self,
        id: fileid3,
        offset: u64,
        count: u32,
    ) -> Result<(Vec<u8>, bool), nfsstat3> {
        if let Some(fh) = self.writes.get(id) {
            let data = self.view.read(fh, offset, count).map_err(|e| status(&e))?;
            let size = self.attr(id).await?.size;
            let eof = offset + data.len() as u64 >= size;
            return Ok((data, eof));
        }
        self.view
            .read_node(id, offset, count)
            .await
            .map_err(|e| status(&e))
    }

    async fn write(&self, id: fileid3, offset: u64, data: &[u8]) -> Result<fattr3, nfsstat3> {
        let fh = self
            .writes
            .handle(&self.view, id)
            .await
            .map_err(|e| status(&e))?;
        self.view.write(fh, offset, data).map_err(|e| status(&e))?;
        self.writes.touch(id);
        self.attr(id).await
    }

    async fn create(
        &self,
        dirid: fileid3,
        filename: &filename3,
        attr: sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        if is_finder_litter(filename.as_ref()) {
            return Err(nfsstat3::NFS3ERR_ACCES);
        }
        let mode = match attr.mode {
            set_mode3::mode(m) => m,
            set_mode3::Void => 0o644,
        };
        let (a, fh) = self
            .view
            .create(dirid, name(filename), mode)
            .await
            .map_err(|e| status(&e))?;
        self.writes.adopt(a.ino, fh);
        Ok((a.ino, self.fattr(&a)))
    }

    async fn create_exclusive(
        &self,
        dirid: fileid3,
        filename: &filename3,
    ) -> Result<fileid3, nfsstat3> {
        if self.view.lookup(dirid, name(filename)).await.is_ok() {
            return Err(nfsstat3::NFS3ERR_EXIST);
        }
        Ok(self.create(dirid, filename, sattr3::default()).await?.0)
    }

    async fn mkdir(
        &self,
        dirid: fileid3,
        dirname: &filename3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let a = self
            .view
            .mkdir(dirid, name(dirname))
            .await
            .map_err(|e| status(&e))?;
        Ok((a.ino, self.fattr(&a)))
    }

    async fn remove(&self, dirid: fileid3, filename: &filename3) -> Result<(), nfsstat3> {
        self.settle().await?;
        match self.view.unlink(dirid, name(filename)).await {
            Err(Error::Io(e)) if e.raw_os_error() == Some(libc::EISDIR) => self
                .view
                .rmdir(dirid, name(filename))
                .await
                .map_err(|e| status(&e)),
            other => other.map_err(|e| status(&e)),
        }
    }

    async fn rename(
        &self,
        from_dirid: fileid3,
        from_filename: &filename3,
        to_dirid: fileid3,
        to_filename: &filename3,
    ) -> Result<(), nfsstat3> {
        self.settle().await?;
        self.view
            .rename(from_dirid, name(from_filename), to_dirid, name(to_filename))
            .await
            .map_err(|e| status(&e))
    }

    async fn readdir(
        &self,
        dirid: fileid3,
        start_after: fileid3,
        max_entries: usize,
    ) -> Result<ReadDirResult, nfsstat3> {
        let entries = self.view.entries(dirid).await.map_err(|e| status(&e))?;
        let start = if start_after == 0 {
            0
        } else {
            entries
                .iter()
                .position(|(_, a)| a.ino == start_after)
                .map(|i| i + 1)
                .ok_or(nfsstat3::NFS3ERR_BAD_COOKIE)?
        };
        let page: Vec<DirEntry> = entries
            .iter()
            .skip(start)
            .take(max_entries)
            .map(|(n, a)| DirEntry {
                fileid: a.ino,
                name: n.as_bytes().into(),
                attr: self.fattr(a),
            })
            .collect();
        let end = start + page.len() >= entries.len();
        Ok(ReadDirResult { entries: page, end })
    }

    async fn symlink(
        &self,
        dirid: fileid3,
        linkname: &filename3,
        symlink: &nfspath3,
        _attr: &sattr3,
    ) -> Result<(fileid3, fattr3), nfsstat3> {
        let target = std::path::Path::new(OsStr::from_bytes(symlink.as_ref()));
        let a = self
            .view
            .symlink(dirid, name(linkname), target)
            .await
            .map_err(|e| status(&e))?;
        Ok((a.ino, self.fattr(&a)))
    }

    async fn readlink(&self, id: fileid3) -> Result<nfspath3, nfsstat3> {
        let bytes = self.view.readlink(id).await.map_err(|e| status(&e))?;
        Ok(bytes.to_vec().into())
    }
}
