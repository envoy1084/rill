//! Change versions, file changes, subtree changes, signatures and change
//! info.

use std::collections::BTreeMap;

use rill_types::objects::change::CopySource;
use rill_types::{
    ChangeInfo, ChangeVersion, FileChange, Principal, Provenance, RepoPathBuf, Signature,
    SubtreeChange,
};

use crate::convert::content::{file_kind_from_proto, merge_terms_from_proto, merge_terms_to_proto};
use crate::convert::primitives::{
    hash_from_proto, hash_to_proto, hashes_from_proto, id_from_proto, id_to_proto,
    opt_hash_from_proto, opt_id_from_proto, path_from_proto, required, timestamp_from_proto,
    timestamp_to_proto, trunk_seq_from_proto, trunk_seq_to_proto,
};
use crate::error::{ConvertError, Result};
use crate::rill::v1 as pb;

impl From<&CopySource> for pb::CopySource {
    fn from(c: &CopySource) -> Self {
        pb::CopySource {
            path: c.path.to_string(),
            version: hash_to_proto(&c.version),
        }
    }
}

impl TryFrom<pb::CopySource> for CopySource {
    type Error = ConvertError;

    fn try_from(c: pb::CopySource) -> Result<Self> {
        Ok(CopySource {
            path: path_from_proto("copy_from.path", &c.path)?,
            version: hash_from_proto("copy_from.version", &c.version)?,
        })
    }
}

impl From<&FileChange> for pb::FileChange {
    fn from(fc: &FileChange) -> Self {
        let change = match fc {
            FileChange::Set {
                content,
                kind,
                copy_from,
            } => pb::file_change::Change::Set(pb::file_change::Set {
                content: Some(pb::ContentRef::from(content)),
                kind: pb::FileKind::from(*kind) as i32,
                copy_from: copy_from.as_ref().map(pb::CopySource::from),
            }),
            FileChange::Delete => pb::file_change::Change::Delete(pb::file_change::Delete {}),
            FileChange::Conflict(merge) => {
                pb::file_change::Change::Conflict(merge_terms_to_proto(merge))
            }
        };
        pb::FileChange {
            change: Some(change),
        }
    }
}

impl From<FileChange> for pb::FileChange {
    fn from(fc: FileChange) -> Self {
        pb::FileChange::from(&fc)
    }
}

impl TryFrom<pb::FileChange> for FileChange {
    type Error = ConvertError;

    fn try_from(fc: pb::FileChange) -> Result<Self> {
        match required("file_change.change", fc.change)? {
            pb::file_change::Change::Set(set) => Ok(FileChange::Set {
                content: required("file_change.set.content", set.content)?.try_into()?,
                kind: file_kind_from_proto("file_change.set.kind", set.kind)?,
                copy_from: set.copy_from.map(CopySource::try_from).transpose()?,
            }),
            pb::file_change::Change::Delete(_) => Ok(FileChange::Delete),
            pb::file_change::Change::Conflict(terms) => {
                Ok(FileChange::Conflict(merge_terms_from_proto(terms)?))
            }
        }
    }
}

impl From<&SubtreeChange> for pb::SubtreeChange {
    fn from(sc: &SubtreeChange) -> Self {
        let change = match sc {
            SubtreeChange::Copy {
                from_path,
                from_version,
            } => pb::subtree_change::Change::Copy(pb::subtree_change::Copy {
                from_path: from_path.to_string(),
                from_version: hash_to_proto(from_version),
            }),
            SubtreeChange::Delete => {
                pb::subtree_change::Change::Delete(pb::subtree_change::Delete {})
            }
        };
        pb::SubtreeChange {
            change: Some(change),
        }
    }
}

impl TryFrom<pb::SubtreeChange> for SubtreeChange {
    type Error = ConvertError;

    fn try_from(sc: pb::SubtreeChange) -> Result<Self> {
        match required("subtree_change.change", sc.change)? {
            pb::subtree_change::Change::Copy(c) => Ok(SubtreeChange::Copy {
                from_path: path_from_proto("subtree_change.copy.from_path", &c.from_path)?,
                from_version: hash_from_proto("subtree_change.copy.from_version", &c.from_version)?,
            }),
            pb::subtree_change::Change::Delete(_) => Ok(SubtreeChange::Delete),
        }
    }
}

/// Convert a path-keyed map, validating each key as a repository path.
fn path_map_from_proto<V, W>(
    field: &'static str,
    map: BTreeMap<String, W>,
) -> Result<BTreeMap<RepoPathBuf, V>>
where
    V: TryFrom<W, Error = ConvertError>,
{
    map.into_iter()
        .map(|(k, v)| Ok((path_from_proto(field, &k)?, V::try_from(v)?)))
        .collect()
}

impl From<&ChangeVersion> for pb::ChangeVersion {
    fn from(cv: &ChangeVersion) -> Self {
        pb::ChangeVersion {
            change_id: id_to_proto(&cv.change_id),
            parents: cv.parents.iter().map(hash_to_proto).collect(),
            predecessor: cv.predecessor.as_ref().map(hash_to_proto),
            subtree_changes: cv
                .subtree_changes
                .iter()
                .map(|(p, c)| (p.to_string(), pb::SubtreeChange::from(c)))
                .collect(),
            file_changes: cv
                .file_changes
                .iter()
                .map(|(p, c)| (p.to_string(), pb::FileChange::from(c)))
                .collect(),
            author: Some(pb::Principal::from(&cv.author)),
            provenance: Some(pb::Provenance::from(&cv.provenance)),
            intent: cv.intent.as_ref().map(id_to_proto),
            message: cv.message.clone(),
            created_at: timestamp_to_proto(cv.created_at),
            extra: cv.extra.clone(),
        }
    }
}

impl From<ChangeVersion> for pb::ChangeVersion {
    fn from(cv: ChangeVersion) -> Self {
        pb::ChangeVersion::from(&cv)
    }
}

impl TryFrom<pb::ChangeVersion> for ChangeVersion {
    type Error = ConvertError;

    fn try_from(cv: pb::ChangeVersion) -> Result<Self> {
        Ok(ChangeVersion {
            change_id: id_from_proto("change_version.change_id", &cv.change_id)?,
            parents: hashes_from_proto("change_version.parents", &cv.parents)?,
            predecessor: opt_hash_from_proto(
                "change_version.predecessor",
                cv.predecessor.as_ref(),
            )?,
            subtree_changes: path_map_from_proto(
                "change_version.subtree_changes",
                cv.subtree_changes,
            )?,
            file_changes: path_map_from_proto("change_version.file_changes", cv.file_changes)?,
            author: required("change_version.author", cv.author)?.try_into()?,
            // A missing provenance is the empty provenance, which keeps
            // minimal clients simple.
            provenance: cv
                .provenance
                .map(Provenance::try_from)
                .transpose()?
                .unwrap_or_default(),
            intent: opt_id_from_proto("change_version.intent", cv.intent.as_ref())?,
            message: cv.message,
            created_at: timestamp_from_proto(cv.created_at),
            extra: cv.extra,
        })
    }
}

impl From<&ChangeInfo> for pb::ChangeInfo {
    fn from(ci: &ChangeInfo) -> Self {
        pb::ChangeInfo {
            hash: hash_to_proto(&ci.hash),
            change_id: id_to_proto(&ci.change_id),
            parents: ci.parents.iter().map(hash_to_proto).collect(),
            predecessor: ci.predecessor.as_ref().map(hash_to_proto),
            author: Some(pb::Principal::from(&ci.author)),
            provenance: Some(pb::Provenance::from(&ci.provenance)),
            intent: ci.intent.as_ref().map(id_to_proto),
            message: ci.message.clone(),
            created_at: timestamp_to_proto(ci.created_at),
            files_changed: ci.files_changed,
            has_conflicts: ci.has_conflicts,
            trunk_seq: ci.trunk_seq.map(trunk_seq_to_proto),
        }
    }
}

impl From<ChangeInfo> for pb::ChangeInfo {
    fn from(ci: ChangeInfo) -> Self {
        pb::ChangeInfo::from(&ci)
    }
}

impl TryFrom<pb::ChangeInfo> for ChangeInfo {
    type Error = ConvertError;

    fn try_from(ci: pb::ChangeInfo) -> Result<Self> {
        Ok(ChangeInfo {
            hash: hash_from_proto("change_info.hash", &ci.hash)?,
            change_id: id_from_proto("change_info.change_id", &ci.change_id)?,
            parents: hashes_from_proto("change_info.parents", &ci.parents)?,
            predecessor: opt_hash_from_proto("change_info.predecessor", ci.predecessor.as_ref())?,
            author: required("change_info.author", ci.author)?.try_into()?,
            provenance: ci
                .provenance
                .map(Provenance::try_from)
                .transpose()?
                .unwrap_or_default(),
            intent: opt_id_from_proto("change_info.intent", ci.intent.as_ref())?,
            message: ci.message,
            created_at: timestamp_from_proto(ci.created_at),
            files_changed: ci.files_changed,
            has_conflicts: ci.has_conflicts,
            trunk_seq: ci.trunk_seq.map(trunk_seq_from_proto),
        })
    }
}

impl From<&Signature> for pb::Signature {
    fn from(s: &Signature) -> Self {
        pb::Signature {
            signer: Some(pb::Principal::from(&s.signer)),
            public_key: s.public_key.clone(),
            signature: s.signature.clone(),
            signed_at: timestamp_to_proto(s.signed_at),
        }
    }
}

impl From<Signature> for pb::Signature {
    fn from(s: Signature) -> Self {
        pb::Signature::from(&s)
    }
}

impl TryFrom<pb::Signature> for Signature {
    type Error = ConvertError;

    fn try_from(s: pb::Signature) -> Result<Self> {
        let signer: Principal = required("signature.signer", s.signer)?.try_into()?;
        Ok(Signature {
            signer,
            public_key: s.public_key,
            signature: s.signature,
            signed_at: timestamp_from_proto(s.signed_at),
        })
    }
}
