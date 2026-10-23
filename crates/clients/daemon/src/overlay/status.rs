//! Status rules: how an overlay entry differs from the base file at its
//! path. Pure.

use rill_types::{FileKind, ObjectHash, RepoPathBuf};

use super::record::{OverlayEntry, OverlayOp};

/// How a path differs from the base.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChangeKind {
    /// Present locally, absent in the base.
    Added,
    /// Present in both with different content or kind.
    Modified,
    /// Absent locally, present in the base.
    Deleted,
    /// Holds unresolved conflict markers from an update.
    Conflict,
}

/// One path that differs from the base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalChange {
    /// The path.
    pub path: RepoPathBuf,
    /// How it differs.
    pub kind: ChangeKind,
}

/// A base file as status sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaseFile {
    /// Digest of the file's bytes (see `cache::local_digest`).
    pub digest: ObjectHash,
    /// File kind.
    pub kind: FileKind,
}

/// How `entry` differs from `base` (the base file at the same path, if
/// any); `None` when it matches.
pub fn classify(entry: &OverlayEntry, base: Option<&BaseFile>) -> Option<ChangeKind> {
    if entry.conflict {
        return Some(ChangeKind::Conflict);
    }
    match (entry.op, base) {
        (OverlayOp::Set { digest, kind, .. }, Some(b)) => {
            (b.digest != digest || b.kind != kind).then_some(ChangeKind::Modified)
        }
        (OverlayOp::Set { .. }, None) => Some(ChangeKind::Added),
        (OverlayOp::Delete, Some(_)) => Some(ChangeKind::Deleted),
        (OverlayOp::Delete, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::ObjectKind;

    fn digest(n: u8) -> ObjectHash {
        ObjectHash::digest(ObjectKind::Blob, &[n])
    }

    fn entry(op: OverlayOp, conflict: bool) -> OverlayEntry {
        OverlayEntry {
            op,
            seq: 1,
            conflict,
        }
    }

    fn set(n: u8, kind: FileKind) -> OverlayOp {
        OverlayOp::Set {
            digest: digest(n),
            kind,
            size: 1,
        }
    }

    #[test]
    fn an_edit_reverted_to_the_base_is_not_a_change() {
        let base = BaseFile {
            digest: digest(1),
            kind: FileKind::Regular,
        };
        assert_eq!(
            classify(&entry(set(1, FileKind::Regular), false), Some(&base)),
            None
        );
        assert_eq!(
            classify(&entry(set(1, FileKind::Executable), false), Some(&base)),
            Some(ChangeKind::Modified)
        );
        assert_eq!(classify(&entry(OverlayOp::Delete, false), None), None);
    }

    #[test]
    fn a_conflict_stays_a_conflict_whatever_the_content() {
        let base = BaseFile {
            digest: digest(1),
            kind: FileKind::Regular,
        };
        assert_eq!(
            classify(&entry(set(1, FileKind::Regular), true), Some(&base)),
            Some(ChangeKind::Conflict)
        );
    }
}
