//! Content references, file kinds and file states.

use rill_types::objects::change::FileState;
use rill_types::{ContentRef, FileKind, Merge};

use crate::convert::primitives::{enum_from_proto, hash_from_proto, hash_to_proto, required};
use crate::error::{ConvertError, Result};
use crate::rill::v1 as pb;

impl From<FileKind> for pb::FileKind {
    fn from(kind: FileKind) -> Self {
        match kind {
            FileKind::Regular => pb::FileKind::Regular,
            FileKind::Executable => pb::FileKind::Executable,
            FileKind::Symlink => pb::FileKind::Symlink,
        }
    }
}

impl TryFrom<pb::FileKind> for FileKind {
    type Error = ConvertError;

    fn try_from(kind: pb::FileKind) -> Result<Self> {
        match kind {
            pb::FileKind::Regular => Ok(FileKind::Regular),
            pb::FileKind::Executable => Ok(FileKind::Executable),
            pb::FileKind::Symlink => Ok(FileKind::Symlink),
            pb::FileKind::Unspecified => Err(ConvertError::BadEnum {
                field: "kind",
                value: 0,
            }),
        }
    }
}

/// Decode a `FileKind` enum field.
pub fn file_kind_from_proto(field: &'static str, value: i32) -> Result<FileKind> {
    let kind: pb::FileKind = enum_from_proto(field, value)?;
    match kind {
        pb::FileKind::Regular => Ok(FileKind::Regular),
        pb::FileKind::Executable => Ok(FileKind::Executable),
        pb::FileKind::Symlink => Ok(FileKind::Symlink),
        // `enum_from_proto` already rejected zero.
        pb::FileKind::Unspecified => Err(ConvertError::BadEnum { field, value }),
    }
}

impl From<&ContentRef> for pb::ContentRef {
    fn from(c: &ContentRef) -> Self {
        let r = match c {
            ContentRef::Inline { bytes } => pb::content_ref::Ref::Inline(bytes.clone()),
            ContentRef::Blob { hash, size } => pb::content_ref::Ref::Blob(pb::BlobRef {
                hash: hash_to_proto(hash),
                size: *size,
            }),
            ContentRef::Chunked { hash, size } => pb::content_ref::Ref::Chunked(pb::BlobRef {
                hash: hash_to_proto(hash),
                size: *size,
            }),
        };
        pb::ContentRef { r#ref: Some(r) }
    }
}

impl From<ContentRef> for pb::ContentRef {
    fn from(c: ContentRef) -> Self {
        pb::ContentRef::from(&c)
    }
}

impl TryFrom<pb::ContentRef> for ContentRef {
    type Error = ConvertError;

    fn try_from(c: pb::ContentRef) -> Result<Self> {
        match required("content_ref.ref", c.r#ref)? {
            pb::content_ref::Ref::Inline(bytes) => Ok(ContentRef::Inline { bytes }),
            pb::content_ref::Ref::Blob(b) => Ok(ContentRef::Blob {
                hash: hash_from_proto("content_ref.blob.hash", &b.hash)?,
                size: b.size,
            }),
            pb::content_ref::Ref::Chunked(b) => Ok(ContentRef::Chunked {
                hash: hash_from_proto("content_ref.chunked.hash", &b.hash)?,
                size: b.size,
            }),
        }
    }
}

/// Encode a file state; absence is a `FileState` with no content.
pub fn file_state_to_proto(state: &FileState) -> pb::FileState {
    match state {
        None => pb::FileState {
            content: None,
            kind: pb::FileKind::Unspecified as i32,
        },
        Some((content, kind)) => pb::FileState {
            content: Some(pb::ContentRef::from(content)),
            kind: pb::FileKind::from(*kind) as i32,
        },
    }
}

/// Decode a file state; a message without content is "absent".
pub fn file_state_from_proto(state: pb::FileState) -> Result<FileState> {
    match state.content {
        None => Ok(None),
        Some(content) => Ok(Some((
            ContentRef::try_from(content)?,
            file_kind_from_proto("file_state.kind", state.kind)?,
        ))),
    }
}

/// Encode a `Merge<FileState>` as its interleaved term list.
pub fn merge_terms_to_proto(merge: &Merge<FileState>) -> pb::MergeTerms {
    pb::MergeTerms {
        terms: merge.iter().map(file_state_to_proto).collect(),
    }
}

/// Decode interleaved terms into a `Merge<FileState>`; the list must have
/// odd length.
pub fn merge_terms_from_proto(terms: pb::MergeTerms) -> Result<Merge<FileState>> {
    let values = terms
        .terms
        .into_iter()
        .map(file_state_from_proto)
        .collect::<Result<Vec<_>>>()?;
    Merge::from_vec(values).map_err(|e| ConvertError::invalid("merge_terms.terms", e))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{ObjectHash, ObjectKind};

    fn h(s: &str) -> ObjectHash {
        ObjectHash::digest(ObjectKind::Blob, s.as_bytes())
    }

    #[test]
    fn content_ref_roundtrip() {
        for c in [
            ContentRef::inline(b"hi".to_vec()),
            ContentRef::Blob {
                hash: h("a"),
                size: 10,
            },
            ContentRef::Chunked {
                hash: h("b"),
                size: 1 << 30,
            },
        ] {
            assert_eq!(ContentRef::try_from(pb::ContentRef::from(&c)).unwrap(), c);
        }
        assert!(matches!(
            ContentRef::try_from(pb::ContentRef { r#ref: None }),
            Err(ConvertError::Missing {
                field: "content_ref.ref"
            })
        ));
    }

    #[test]
    fn merge_terms_roundtrip() {
        let base: FileState = Some((ContentRef::inline(b"base".to_vec()), FileKind::Regular));
        let left: FileState = Some((ContentRef::inline(b"left".to_vec()), FileKind::Executable));
        let right: FileState = None;
        let m = Merge::three_way(base, left, right);
        let wire = merge_terms_to_proto(&m);
        assert_eq!(wire.terms.len(), 3);
        assert_eq!(merge_terms_from_proto(wire).unwrap(), m);
    }

    #[test]
    fn merge_terms_rejects_even_length() {
        let wire = pb::MergeTerms {
            terms: vec![file_state_to_proto(&None), file_state_to_proto(&None)],
        };
        assert!(matches!(
            merge_terms_from_proto(wire),
            Err(ConvertError::Invalid {
                field: "merge_terms.terms",
                ..
            })
        ));
    }
}
