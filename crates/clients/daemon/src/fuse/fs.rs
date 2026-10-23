//! The kernel's calls, answered from [`View`]: each runs its async work
//! on the daemon's runtime from the FUSE worker thread that received it.

use std::ffi::OsStr;
use std::future::Future;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use fuser::{
    Errno, FileHandle, Filesystem, FopenFlags, Generation, INodeNo, InitFlags, KernelConfig,
    OpenAccMode, OpenFlags, RenameFlags, ReplyAttr, ReplyCreate, ReplyData, ReplyDirectory,
    ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request, TimeOrNow, WriteFlags,
};

use super::view::View;
use crate::error::Error;

/// How long the kernel may cache entries and attributes.
const TTL: Duration = Duration::from_secs(1);

/// The view as a FUSE filesystem.
pub struct Fs(pub Arc<View>);

fn errno(e: &Error) -> Errno {
    match e {
        Error::NotFound(_) => Errno::ENOENT,
        Error::Invalid(_) => Errno::EINVAL,
        Error::Io(io) => match io.raw_os_error() {
            Some(code) => Errno::from_i32(code),
            None => Errno::EIO,
        },
        other => {
            tracing::warn!(error = %other, "view operation failed");
            Errno::EIO
        }
    }
}

impl Fs {
    fn block<T>(&self, f: impl Future<Output = crate::error::Result<T>>) -> Result<T, Errno> {
        self.0.rt.block_on(f).map_err(|e| errno(&e))
    }
}

macro_rules! answer {
    ($reply:ident, $result:expr, |$v:ident| $ok:expr) => {
        match $result {
            Ok($v) => $ok,
            Err(e) => $reply.error(e),
        }
    };
}

impl Filesystem for Fs {
    fn init(&mut self, _req: &Request, config: &mut KernelConfig) -> std::io::Result<()> {
        let passthrough = config.capabilities().contains(InitFlags::FUSE_PASSTHROUGH)
            && config.add_capabilities(InitFlags::FUSE_PASSTHROUGH).is_ok()
            && config.set_max_stack_depth(2).is_ok();
        self.0.passthrough.store(passthrough, Ordering::Relaxed);
        tracing::info!(passthrough, "view mounted");
        Ok(())
    }

    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        answer!(reply, self.block(self.0.lookup(parent.0, name)), |attr| {
            reply.entry(&TTL, &attr, Generation(0))
        })
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, fh: Option<FileHandle>, reply: ReplyAttr) {
        answer!(
            reply,
            self.block(self.0.getattr(ino.0, fh.map(|f| f.0))),
            |attr| { reply.attr(&TTL, &attr) }
        )
    }

    fn setattr(
        &self,
        _req: &Request,
        ino: INodeNo,
        mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>,
        _ctime: Option<std::time::SystemTime>,
        fh: Option<FileHandle>,
        _crtime: Option<std::time::SystemTime>,
        _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<fuser::BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        let result = self.block(self.0.setattr(ino.0, fh.map(|f| f.0), size, mode));
        answer!(reply, result, |attr| reply.attr(&TTL, &attr))
    }

    fn readlink(&self, _req: &Request, ino: INodeNo, reply: ReplyData) {
        answer!(reply, self.block(self.0.readlink(ino.0)), |bytes| reply
            .data(&bytes))
    }

    fn mkdir(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        answer!(reply, self.block(self.0.mkdir(parent.0, name)), |attr| {
            reply.entry(&TTL, &attr, Generation(0))
        })
    }

    fn unlink(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        answer!(reply, self.block(self.0.unlink(parent.0, name)), |_v| reply
            .ok())
    }

    fn rmdir(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        answer!(reply, self.block(self.0.rmdir(parent.0, name)), |_v| reply
            .ok())
    }

    fn symlink(
        &self,
        _req: &Request,
        parent: INodeNo,
        link_name: &OsStr,
        target: &Path,
        reply: ReplyEntry,
    ) {
        answer!(
            reply,
            self.block(self.0.symlink(parent.0, link_name, target)),
            |attr| { reply.entry(&TTL, &attr, Generation(0)) }
        )
    }

    fn rename(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        newparent: INodeNo,
        newname: &OsStr,
        _flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
        let result = self.block(self.0.rename(parent.0, name, newparent.0, newname));
        answer!(reply, result, |_v| reply.ok())
    }

    fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        let write = flags.acc_mode() != OpenAccMode::O_RDONLY;
        let truncate = flags.0 & libc::O_TRUNC != 0;
        let opened = match self.block(self.0.open(ino.0, write, truncate)) {
            Ok(o) => o,
            Err(e) => return reply.error(e),
        };
        let fh = FileHandle(opened.fh);
        if let (Some(file), true) = (opened.backing, self.0.passthrough.load(Ordering::Relaxed)) {
            match reply.open_backing(&file) {
                Ok(backing) => {
                    return reply.opened_passthrough(fh, FopenFlags::FOPEN_KEEP_CACHE, &backing)
                }
                Err(e) => {
                    tracing::info!(error = %e, "passthrough refused; serving reads");
                    self.0.passthrough.store(false, Ordering::Relaxed);
                }
            }
        }
        let flags = if write {
            FopenFlags::empty()
        } else {
            FopenFlags::FOPEN_KEEP_CACHE
        };
        reply.opened(fh, flags)
    }

    fn read(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        reply: ReplyData,
    ) {
        answer!(
            reply,
            self.0.read(fh.0, offset, size).map_err(|e| errno(&e)),
            |data| { reply.data(&data) }
        )
    }

    fn write(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        data: &[u8],
        _write_flags: WriteFlags,
        _flags: OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        reply: ReplyWrite,
    ) {
        answer!(
            reply,
            self.0.write(fh.0, offset, data).map_err(|e| errno(&e)),
            |n| { reply.written(n) }
        )
    }

    fn flush(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _lock_owner: fuser::LockOwner,
        reply: ReplyEmpty,
    ) {
        answer!(reply, self.block(self.0.flush(fh.0)), |_v| reply.ok())
    }

    fn fsync(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        answer!(reply, self.block(self.0.flush(fh.0)), |_v| reply.ok())
    }

    fn release(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        _lock_owner: Option<fuser::LockOwner>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        answer!(reply, self.block(self.0.release(fh.0)), |_v| reply.ok())
    }

    fn create(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        answer!(
            reply,
            self.block(self.0.create(parent.0, name, mode)),
            |created| {
                let (attr, fh) = created;
                reply.created(
                    &TTL,
                    &attr,
                    Generation(0),
                    FileHandle(fh),
                    FopenFlags::empty(),
                )
            }
        )
    }

    fn opendir(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        answer!(reply, self.block(self.0.opendir(ino.0)), |fh| {
            reply.opened(FileHandle(fh), FopenFlags::empty())
        })
    }

    fn readdir(
        &self,
        _req: &Request,
        ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let handles = self.0.handles.lock();
        let Some(super::handles::Handle::Dir(entries)) = handles.get(&fh.0) else {
            return reply.error(Errno::EBADF);
        };
        let parent = ino.0;
        let dots = [
            (ino.0, true, ".".to_string()),
            (parent, true, "..".to_string()),
        ];
        for (i, (child, is_dir, name)) in dots
            .iter()
            .chain(entries.iter())
            .enumerate()
            .skip(offset as usize)
        {
            let kind = if *is_dir {
                fuser::FileType::Directory
            } else {
                fuser::FileType::RegularFile
            };
            if reply.add(INodeNo(*child), (i + 1) as u64, kind, name) {
                break;
            }
        }
        reply.ok()
    }

    fn releasedir(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        reply: ReplyEmpty,
    ) {
        self.0.handles.lock().remove(&fh.0);
        reply.ok()
    }

    fn statfs(&self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        reply.statfs(1 << 32, 1 << 31, 1 << 31, 1 << 32, 1 << 31, 4096, 255, 4096)
    }
}
