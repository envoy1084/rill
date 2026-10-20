//! Filesystem primitives for the working copy: reading and writing
//! entries by kind, stat for the fast path, and the local hash.

use std::path::Path;
use std::time::UNIX_EPOCH;

use bytes::Bytes;
use rill_types::{FileKind, ObjectHash, ObjectKind};

use crate::error::Result;

/// Size and mtime of a path, or `None` if it does not exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stat {
    /// Size in bytes (the link target length for symlinks).
    pub size: u64,
    /// Modification time in epoch milliseconds.
    pub mtime_ms: i64,
    /// True for a symlink.
    pub is_symlink: bool,
}

/// Stat without following symlinks.
pub fn stat(path: &Path) -> Result<Option<Stat>> {
    match std::fs::symlink_metadata(path) {
        Ok(md) => {
            if md.is_dir() {
                return Ok(None);
            }
            let mtime_ms = md
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Ok(Some(Stat {
                size: md.len(),
                mtime_ms,
                is_symlink: md.file_type().is_symlink(),
            }))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// Read a file's content and kind. A symlink's content is its target.
pub fn read_entry(path: &Path) -> Result<(Bytes, FileKind)> {
    let md = std::fs::symlink_metadata(path)?;
    if md.file_type().is_symlink() {
        let target = std::fs::read_link(path)?;
        let bytes = Bytes::from(target.to_string_lossy().into_owned());
        return Ok((bytes, FileKind::Symlink));
    }
    let bytes = Bytes::from(std::fs::read(path)?);
    let kind = if is_executable(&md) {
        FileKind::Executable
    } else {
        FileKind::Regular
    };
    Ok((bytes, kind))
}

/// Write content with the given kind, creating parent directories and
/// replacing whatever was there.
pub fn write_entry(path: &Path, bytes: &[u8], kind: FileKind) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    remove_entry(path)?;
    match kind {
        FileKind::Symlink => write_symlink(path, bytes),
        FileKind::Regular | FileKind::Executable => {
            std::fs::write(path, bytes)?;
            set_executable(path, kind.is_executable())
        }
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

/// The digest used to compare local content with the manifest.
pub fn local_hash(bytes: &[u8]) -> ObjectHash {
    ObjectHash::digest(ObjectKind::Blob, bytes)
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
    let mode = if executable {
        perms.mode() | 0o111
    } else {
        perms.mode() & !0o111
    };
    perms.set_mode(mode);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_executable(_path: &Path, _executable: bool) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn write_symlink(path: &Path, target: &[u8]) -> Result<()> {
    let target = String::from_utf8_lossy(target).into_owned();
    std::os::unix::fs::symlink(target, path)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_symlink(path: &Path, target: &[u8]) -> Result<()> {
    // Without symlink support the target is stored as a plain file so
    // nothing is lost; status will report it as a regular file.
    std::fs::write(path, target)?;
    Ok(())
}
