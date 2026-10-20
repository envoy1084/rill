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
pub mod merge;
pub mod objects;
pub mod path;
pub mod principal;
#[cfg(feature = "testing")]
pub mod testing;
pub mod time;

pub use encoding::{decode, encode, Encodable, FormatVersion};
pub use error::{Error, Result};
pub use hash::{ObjectHash, ObjectKind};
pub use ids::{ChangeId, IntentId, OperationId, PrincipalId, RepoId, WorkspaceId};
pub use merge::Merge;
pub use objects::blob::{BlobManifest, ContentRef, INLINE_THRESHOLD};
pub use objects::change::{
    ChangeInfo, ChangeVersion, ChangeVersionHash, FileChange, FileKind, Provenance, Signature,
    SubtreeChange,
};
pub use objects::intent::{Intent, IntentKind, IntentStatus, RiskClass};
pub use objects::refs::{RefName, RefUpdate, TrunkSeq};
pub use objects::review::{
    Comment, Label, LabelRange, LabelVote, ReviewState, ReviewStatus, Thread, ThreadAnchor,
};
pub use objects::tree::{EntryKind, Tree, TreeEntry, TreeHash};
pub use objects::workspace::{Operation, OperationKind, SparseProfile, Workspace};
pub use path::{RepoPath, RepoPathBuf};
pub use principal::{Principal, PrincipalKind};
pub use time::Timestamp;
