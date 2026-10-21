//! Core object model for Rill.
//!
//! Everything in this crate is plain data: hashes and identifiers, the
//! immutable objects that make up a repository (blobs, trees, change
//! versions), the mutable records that point at them (refs, workspaces,
//! intents), and the canonical encoding that gives every immutable object a
//! stable BLAKE3 identity.
//!
//! The crate has no I/O and no async; it is shared by the server, the client
//! and every tool that speaks the Rill API.

pub mod encoding;
pub mod error;
pub mod hash;
pub mod ids;
pub mod limits;
pub mod merge;
pub mod objects;
pub mod path;
pub mod principal;
pub mod stamp;
#[cfg(feature = "testing")]
pub mod testing;
pub mod time;

pub use encoding::{
    decode, decode_record, decode_tagged, encode, encode_record, encode_tagged, Encodable, Record,
    Schema,
};
pub use error::{Error, Result};
pub use hash::{ObjectHash, ObjectKind};
pub use ids::{
    ChangeId, IntentId, JobId, OperationId, PrincipalId, RepoId, SessionId, TokenId, WorkspaceId,
};
pub use merge::Merge;
pub use objects::blob::{BlobManifest, ContentRef, INLINE_THRESHOLD};
pub use objects::change::{
    ChangeInfo, ChangeVersion, ChangeVersionHash, FileChange, FileKind, Provenance, Signature,
    SubtreeChange,
};
pub use objects::event::{Event, EventKind};
pub use objects::intent::{Intent, IntentKind, IntentStatus, RiskClass};
pub use objects::land_request::{
    LandRequest, LandRequestRecord, LandStatus, LandTicket, Lane, VerifyMode,
};
pub use objects::landing::Landing;
pub use objects::refs::{RefName, RefUpdate, TrunkSeq};
pub use objects::review::{
    ChangeRecord, Comment, Label, LabelRange, LabelVote, ReviewState, ReviewStatus, Thread,
    ThreadAnchor, ThreadHeader,
};
pub use objects::tree::{
    DirSummary, EntryKind, Tree, TreeChild, TreeEntry, TreeHash, TreeNode, SHARD_ENTRIES,
};
pub use objects::workspace::{Operation, OperationKind, SparseProfile, Workspace};
pub use path::{RepoPath, RepoPathBuf};
pub use principal::{Principal, PrincipalKind};
pub use stamp::Versionstamp;
pub use time::Timestamp;
