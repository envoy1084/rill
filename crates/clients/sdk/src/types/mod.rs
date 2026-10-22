//! SDK-only types: values the API exposes that have no `rill-types`
//! counterpart, plus typed inputs to write RPCs.

mod access;
mod admin;
mod diff;
mod edit;
mod event;
mod intent;
mod page;
mod repo;
mod review;
mod root;
mod search;
mod trunk;
mod workspace;

pub use access::{AccessCheck, ExpandedRelation, Relation, RelationFilter};
pub use admin::{Capability, ComponentHealth, Health, IssuedToken, Policy, TokenRequest, WhoAmI};
pub use diff::{DiffFormat, DiffLine, DiffOp, DiffOptions, DiffResult, FileDiff, Hunk};
pub(crate) use edit::set_content_proto;
pub use edit::{ApplyOptions, Edit, EditBase, MergeOptions, Resolution, VersionResult};
pub use event::{EventQuery, LoggedEvent};
pub use intent::{FieldUpdate, IntentFilter, IntentUpdate, NewIntent, ScoredIntent};
pub use page::Page;
pub use repo::{
    BlameLine, Derived, LandEvent, LogEntry, LogPage, OrgInfo, PathHistoryEntry, PathHistoryPage,
    RefEvent, RefInfo, RepoInfo,
};
pub use review::{
    status_name, LandProgress, LandStatus, LandTicket, NewComment, NewThread, Reply, ReviewEvent,
    ReviewEventKind, ReviewFilter, ReviewInfo, SubmitDecision,
};
pub use root::{LogStart, Root};
pub use search::{GrepHit, GrepQuery, GrepResult};
pub use trunk::{
    BisectResult, DerivedBytes, Landing, NewVerification, RevertResult, RevertSelector,
    TrunkStatus, VerificationRecord, VerificationReport, VerificationStatus,
};
pub use workspace::{NewChange, NewChangeResult, UpdateBaseResult, WorkspaceFilter, WorkspaceOp};
