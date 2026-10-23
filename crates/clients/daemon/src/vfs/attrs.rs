//! Node attributes, independent of the kernel interface that reports
//! them. Modification times are synthetic and derived from the content
//! hash, so the same content always has the same mtime and build tools
//! never rebuild for a checkout or an update that did not change a file.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rill_types::{FileKind, ObjectHash};

/// Start of the synthetic mtime range (2001-09-09).
const EPOCH_OFFSET: u64 = 1_000_000_000;
/// Width of the range, about 30 years.
const RANGE: u64 = 1_000_000_000;

/// Who owns every file: the daemon's user.
#[derive(Debug, Clone, Copy)]
pub struct Owner {
    pub uid: u32,
    pub gid: u32,
}

impl Owner {
    /// The current process's user and group.
    pub fn current() -> Self {
        // SAFETY: getuid and getgid cannot fail and touch no memory.
        unsafe {
            Owner {
                uid: libc::getuid(),
                gid: libc::getgid(),
            }
        }
    }
}

/// The mtime of content with this hash.
pub fn synthetic_mtime(hash: &ObjectHash) -> SystemTime {
    let b = hash.as_bytes();
    let n = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
    UNIX_EPOCH + Duration::from_secs(EPOCH_OFFSET + n % RANGE)
}

/// What kind of node an attribute set describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrKind {
    Dir,
    File,
    Symlink,
}

/// Attributes of a node, as every view front end reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attr {
    pub ino: u64,
    pub size: u64,
    pub kind: AttrKind,
    pub perm: u16,
    pub mtime: SystemTime,
    pub uid: u32,
    pub gid: u32,
}

/// Attributes of a file.
pub fn file_attr(ino: u64, size: u64, kind: FileKind, hash: &ObjectHash, owner: Owner) -> Attr {
    let (kind, perm) = match kind {
        FileKind::Regular => (AttrKind::File, 0o644),
        FileKind::Executable => (AttrKind::File, 0o755),
        FileKind::Symlink => (AttrKind::Symlink, 0o777),
    };
    Attr {
        ino,
        size,
        kind,
        perm,
        mtime: synthetic_mtime(hash),
        uid: owner.uid,
        gid: owner.gid,
    }
}

/// Attributes of a directory.
pub fn dir_attr(ino: u64, owner: Owner) -> Attr {
    Attr {
        ino,
        size: 4096,
        kind: AttrKind::Dir,
        perm: 0o755,
        mtime: UNIX_EPOCH + Duration::from_secs(EPOCH_OFFSET),
        uid: owner.uid,
        gid: owner.gid,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::ObjectKind;

    #[test]
    fn mtimes_follow_content_and_stay_in_range() {
        let a = ObjectHash::digest(ObjectKind::Blob, b"a");
        let b = ObjectHash::digest(ObjectKind::Blob, b"b");
        assert_eq!(synthetic_mtime(&a), synthetic_mtime(&a));
        assert_ne!(synthetic_mtime(&a), synthetic_mtime(&b));
        let secs = synthetic_mtime(&a)
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        assert!((EPOCH_OFFSET..EPOCH_OFFSET + RANGE).contains(&secs));
    }
}
