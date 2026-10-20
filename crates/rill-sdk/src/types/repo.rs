//! Repository, history, ref and trunk records without a `rill-types`
//! counterpart.

use bytes::Bytes;
use rill_proto::convert::{
    hash_from_proto, id_from_proto, opt_hash_from_proto, ref_name_from_proto, timestamp_from_proto,
    trunk_seq_from_proto,
};
use rill_proto::rill::v1 as pb;
use rill_types::{
    ChangeId, ChangeInfo, ChangeVersionHash, FileChange, Principal, RefName, RepoId, RepoPathBuf,
    Timestamp, TrunkSeq,
};

use crate::error::Result;

/// A repository record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    /// Stable repository id.
    pub id: RepoId,
    /// Repository name.
    pub name: String,
    /// Free-form description.
    pub description: Option<String>,
    /// Creation time.
    pub created_at: Timestamp,
    /// Sequence number of the trunk head.
    pub trunk_head_seq: TrunkSeq,
    /// Paths Windows cannot create are rejected.
    pub windows_safe: bool,
}

impl TryFrom<pb::Repo> for RepoInfo {
    type Error = crate::error::Error;

    fn try_from(r: pb::Repo) -> Result<Self> {
        Ok(RepoInfo {
            id: id_from_proto("repo.id", &r.id)?,
            name: r.name,
            description: r.description,
            created_at: timestamp_from_proto(r.created_at),
            trunk_head_seq: trunk_seq_from_proto(r.trunk_head_seq),
            windows_safe: r.windows_safe,
        })
    }
}

/// One page of a history walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogPage {
    /// Entries in walk order.
    pub entries: Vec<ChangeInfo>,
    /// Pass as the next start to continue, if more history exists.
    pub next_start: Option<ChangeVersionHash>,
}

/// One version in a path's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathHistoryEntry {
    /// The version that touched the path.
    pub info: ChangeInfo,
    /// The path as of this version (differs across renames).
    pub path: RepoPathBuf,
    /// What the version did to the path.
    pub change: FileChange,
}

impl TryFrom<pb::PathHistoryEntry> for PathHistoryEntry {
    type Error = crate::error::Error;

    fn try_from(e: pb::PathHistoryEntry) -> Result<Self> {
        Ok(PathHistoryEntry {
            info: rill_proto::convert::required("path_history_entry.info", e.info)?.try_into()?,
            path: RepoPathBuf::new(&e.path)?,
            change: rill_proto::convert::required("path_history_entry.change", e.change)?
                .try_into()?,
        })
    }
}

/// One page of a path history walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathHistoryPage {
    /// Entries in walk order.
    pub entries: Vec<PathHistoryEntry>,
    /// Pass as the next start to continue, if more history exists.
    pub next_start: Option<ChangeVersionHash>,
}

/// Attribution of one line of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlameLine {
    /// One-based line number in the blamed version.
    pub line_no: u32,
    /// The version that last changed the line.
    pub version: ChangeVersionHash,
    /// Author of that version.
    pub author: Principal,
    /// Trunk sequence of that version, if landed.
    pub trunk_seq: Option<TrunkSeq>,
    /// The line's bytes, without a terminator.
    pub text: Bytes,
}

impl TryFrom<pb::BlameLine> for BlameLine {
    type Error = crate::error::Error;

    fn try_from(l: pb::BlameLine) -> Result<Self> {
        Ok(BlameLine {
            line_no: l.line_no,
            version: hash_from_proto("blame_line.version", &l.version)?,
            author: rill_proto::convert::required("blame_line.author", l.author)?.try_into()?,
            trunk_seq: l.trunk_seq.map(trunk_seq_from_proto),
            text: l.text,
        })
    }
}

/// A named pointer and its target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefInfo {
    /// The ref name.
    pub name: RefName,
    /// Current target.
    pub target: ChangeVersionHash,
    /// For trunk, the sequence number of the head.
    pub trunk_seq: Option<TrunkSeq>,
}

impl TryFrom<pb::Ref> for RefInfo {
    type Error = crate::error::Error;

    fn try_from(r: pb::Ref) -> Result<Self> {
        Ok(RefInfo {
            name: ref_name_from_proto("ref.name", &r.name)?,
            target: hash_from_proto("ref.target", &r.target)?,
            trunk_seq: r.trunk_seq.map(trunk_seq_from_proto),
        })
    }
}

/// A ref moved, appeared or disappeared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefEvent {
    /// The ref name.
    pub name: RefName,
    /// Previous target; `None` when the ref was created.
    pub old: Option<ChangeVersionHash>,
    /// New target; `None` when the ref was deleted.
    pub new: Option<ChangeVersionHash>,
    /// When the move happened.
    pub at: Timestamp,
}

impl TryFrom<pb::RefEvent> for RefEvent {
    type Error = crate::error::Error;

    fn try_from(e: pb::RefEvent) -> Result<Self> {
        Ok(RefEvent {
            name: ref_name_from_proto("ref_event.name", &e.name)?,
            old: opt_hash_from_proto("ref_event.old", e.old.as_ref())?,
            new: opt_hash_from_proto("ref_event.new", e.new.as_ref())?,
            at: timestamp_from_proto(e.at),
        })
    }
}

/// A version landed on trunk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandEvent {
    /// Position on trunk.
    pub seq: TrunkSeq,
    /// The landed version.
    pub hash: ChangeVersionHash,
    /// Stable identity of the landed change.
    pub change_id: ChangeId,
    /// Who landed it.
    pub landed_by: Principal,
    /// When it landed.
    pub at: Timestamp,
    /// Set when this landing reverts an earlier sequence.
    pub reverts_seq: Option<TrunkSeq>,
}

impl TryFrom<pb::LandEvent> for LandEvent {
    type Error = crate::error::Error;

    fn try_from(e: pb::LandEvent) -> Result<Self> {
        Ok(LandEvent {
            seq: trunk_seq_from_proto(e.seq),
            hash: hash_from_proto("land_event.hash", &e.hash)?,
            change_id: id_from_proto("land_event.change_id", &e.change_id)?,
            landed_by: rill_proto::convert::required("land_event.landed_by", e.landed_by)?
                .try_into()?,
            at: timestamp_from_proto(e.at),
            reverts_seq: e.reverts_seq.map(trunk_seq_from_proto),
        })
    }
}

/// The result of waiting for derived data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Derived {
    /// The data is ready.
    Ready(Bytes),
    /// The wait timed out before the data was computed.
    Pending,
}
