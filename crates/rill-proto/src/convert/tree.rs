//! Tree and tree entry conversions.

use rill_types::objects::tree::DirSummary;
use rill_types::{EntryKind, Tree, TreeEntry};

use crate::convert::content::file_kind_from_proto;
use crate::convert::primitives::{hash_from_proto, hash_to_proto, required};
use crate::error::{ConvertError, Result};
use crate::rill::v1 as pb;

impl From<DirSummary> for pb::DirSummary {
    fn from(s: DirSummary) -> Self {
        pb::DirSummary {
            file_count: s.file_count,
            dir_count: s.dir_count,
            total_size: s.total_size,
        }
    }
}

impl From<pb::DirSummary> for DirSummary {
    fn from(s: pb::DirSummary) -> Self {
        DirSummary {
            file_count: s.file_count,
            dir_count: s.dir_count,
            total_size: s.total_size,
        }
    }
}

impl From<&TreeEntry> for pb::TreeEntry {
    fn from(e: &TreeEntry) -> Self {
        let kind = match &e.kind {
            EntryKind::File { content, kind } => pb::tree_entry::Kind::File(pb::tree_entry::File {
                content: Some(pb::ContentRef::from(content)),
                kind: pb::FileKind::from(*kind) as i32,
            }),
            EntryKind::Dir { hash, summary } => pb::tree_entry::Kind::Dir(pb::tree_entry::Dir {
                hash: hash_to_proto(hash),
                summary: Some(pb::DirSummary::from(*summary)),
            }),
        };
        pb::TreeEntry { kind: Some(kind) }
    }
}

impl From<TreeEntry> for pb::TreeEntry {
    fn from(e: TreeEntry) -> Self {
        pb::TreeEntry::from(&e)
    }
}

impl TryFrom<pb::TreeEntry> for TreeEntry {
    type Error = ConvertError;

    fn try_from(e: pb::TreeEntry) -> Result<Self> {
        let kind = match required("tree_entry.kind", e.kind)? {
            pb::tree_entry::Kind::File(f) => EntryKind::File {
                content: required("tree_entry.file.content", f.content)?.try_into()?,
                kind: file_kind_from_proto("tree_entry.file.kind", f.kind)?,
            },
            pb::tree_entry::Kind::Dir(d) => EntryKind::Dir {
                hash: hash_from_proto("tree_entry.dir.hash", &d.hash)?,
                summary: d.summary.map(DirSummary::from).unwrap_or_default(),
            },
        };
        Ok(TreeEntry { kind })
    }
}

impl From<&Tree> for pb::Tree {
    fn from(t: &Tree) -> Self {
        pb::Tree {
            entries: t
                .entries
                .iter()
                .map(|(name, e)| (name.clone(), pb::TreeEntry::from(e)))
                .collect(),
        }
    }
}

impl From<Tree> for pb::Tree {
    fn from(t: Tree) -> Self {
        pb::Tree::from(&t)
    }
}

impl TryFrom<pb::Tree> for Tree {
    type Error = ConvertError;

    fn try_from(t: pb::Tree) -> Result<Self> {
        let entries = t
            .entries
            .into_iter()
            .map(|(name, e)| Ok((name, TreeEntry::try_from(e)?)))
            .collect::<Result<_>>()?;
        Ok(Tree { entries })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{ContentRef, Encodable, FileKind};

    #[test]
    fn tree_roundtrip() {
        let mut inner = Tree::empty();
        inner.insert_file(
            "a.txt",
            ContentRef::inline(b"xyz".to_vec()),
            FileKind::Regular,
        );
        inner.insert_file(
            "run.sh",
            ContentRef::inline(b"#!".to_vec()),
            FileKind::Executable,
        );
        let mut outer = Tree::empty();
        outer.insert_dir("src", inner.hash(), inner.summary());
        outer.insert_file(
            "big",
            ContentRef::Chunked {
                hash: rill_types::ObjectHash::digest(rill_types::ObjectKind::BlobManifest, b"m"),
                size: 5_000_000,
            },
            FileKind::Regular,
        );
        let wire = pb::Tree::from(&outer);
        let back = Tree::try_from(wire).unwrap();
        assert_eq!(back, outer);
        assert_eq!(back.hash(), outer.hash());
    }

    #[test]
    fn entry_without_kind_is_rejected() {
        assert!(matches!(
            TreeEntry::try_from(pb::TreeEntry { kind: None }),
            Err(ConvertError::Missing {
                field: "tree_entry.kind"
            })
        ));
    }
}
