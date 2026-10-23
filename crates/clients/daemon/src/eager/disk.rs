//! Files of an eager mount on disk: reading and writing entries by kind,
//! stat, and the mapping between repository and filesystem paths.
//! Blocking.

use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use bytes::Bytes;
use rill_types::{FileKind, RepoPath, RepoPathBuf};

use crate::cache::local_digest;
use crate::error::{Error, Result};
use crate::state::FileStat;

/// What `lstat` says of a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Found {
    /// Nothing there.
    Absent,
    /// A directory.
    Dir,
    /// A file or symlink of this size, mtime and kind.
    File {
        /// Byte length (the target's length for a symlink).
        size: u64,
        /// Nanoseconds since the epoch.
        mtime_ns: i64,
        /// File kind.
        kind: FileKind,
    },
}

/// Stat without following symlinks.
pub fn stat(path: &Path) -> Result<Found> {
    let md = match std::fs::symlink_metadata(path) {
        Ok(md) => md,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Found::Absent),
        Err(e) => return Err(e.into()),
    };
    if md.is_dir() {
        return Ok(Found::Dir);
    }
    let mtime_ns = md
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos() as i64);
    let kind = if md.file_type().is_symlink() {
        FileKind::Symlink
    } else if is_executable(&md) {
        FileKind::Executable
    } else {
        FileKind::Regular
    };
    Ok(Found::File {
        size: md.len(),
        mtime_ns,
        kind,
    })
}

/// A file's content and kind; a symlink's content is its target.
pub fn read_entry(path: &Path) -> Result<(Bytes, FileKind)> {
    let md = std::fs::symlink_metadata(path)?;
    if md.file_type().is_symlink() {
        let target = std::fs::read_link(path)?;
        return Ok((
            Bytes::from(target.to_string_lossy().into_owned()),
            FileKind::Symlink,
        ));
    }
    let kind = if is_executable(&md) {
        FileKind::Executable
    } else {
        FileKind::Regular
    };
    Ok((Bytes::from(std::fs::read(path)?), kind))
}

/// Write content of a kind, creating parents and replacing what is there,
/// and return what stat then sees.
pub fn write_entry(path: &Path, bytes: &[u8], kind: FileKind) -> Result<FileStat> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    remove_entry(path)?;
    match kind {
        FileKind::Symlink => write_symlink(path, bytes)?,
        FileKind::Regular | FileKind::Executable => {
            std::fs::write(path, bytes)?;
            set_executable(path, kind.is_executable())?;
        }
    }
    match stat(path)? {
        Found::File { size, mtime_ns, .. } => Ok(FileStat {
            size,
            mtime_ns,
            digest: local_digest(bytes),
            kind,
        }),
        _ => Err(Error::Conflict(format!(
            "{} changed while it was written",
            path.display()
        ))),
    }
}

/// Remove a file or symlink if present; directories are left alone.
pub fn remove_entry(path: &Path) -> Result<()> {
    match std::fs::symlink_metadata(path) {
        Ok(md) if !md.is_dir() => Ok(std::fs::remove_file(path)?),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Remove now-empty parent directories up to (not including) `root`.
pub fn prune_empty_dirs(root: &Path, path: &Path) {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d == root || std::fs::remove_dir(d).is_err() {
            break;
        }
        dir = d.parent();
    }
}

/// A mount-relative OS path as a repository path.
pub fn to_repo_path(rel: &Path) -> Result<RepoPathBuf> {
    let mut out = RepoPathBuf::root();
    for component in rel.components() {
        let s = component
            .as_os_str()
            .to_str()
            .ok_or_else(|| Error::Invalid(format!("non-UTF-8 path {}", rel.display())))?;
        out.push(s)?;
    }
    Ok(out)
}

/// A repository path under `root`.
pub fn to_fs_path(root: &Path, path: &RepoPath) -> PathBuf {
    let mut out = root.to_path_buf();
    for c in path.components() {
        out.push(c);
    }
    out
}

#[cfg(unix)]
fn is_executable(md: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    md.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(_md: &std::fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn set_executable(path: &Path, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    let mode = perms.mode();
    perms.set_mode(if executable {
        mode | ((mode & 0o444) >> 2)
    } else {
        mode & !0o111
    });
    Ok(std::fs::set_permissions(path, perms)?)
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn write_symlink(path: &Path, target: &[u8]) -> Result<()> {
    let target = std::str::from_utf8(target)
        .map_err(|_| Error::Invalid(format!("{}: symlink target is not UTF-8", path.display())))?;
    Ok(std::os::unix::fs::symlink(target, path)?)
}

#[cfg(not(unix))]
fn write_symlink(path: &Path, target: &[u8]) -> Result<()> {
    Ok(std::fs::write(path, target)?)
}
