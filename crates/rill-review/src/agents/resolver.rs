//! The conflict-resolver interface, for agents that propose resolutions
//! to `Merge` terms. Orchestration lives with the lander; this crate only
//! fixes the shape so reviewers and resolvers share one agent model.

use async_trait::async_trait;
use bytes::Bytes;
use rill_types::objects::change::FileState;
use rill_types::{ChangeId, FileKind, Merge, RepoPathBuf};

use crate::error::Result;

/// One conflicted path with its terms and, where readable, their text.
#[derive(Debug, Clone)]
pub struct ConflictTerms {
    /// The change in conflict.
    pub change_id: ChangeId,
    /// The conflicted path.
    pub path: RepoPathBuf,
    /// The stored terms: each side's file state.
    pub terms: Merge<FileState>,
    /// Text of each term in `terms.iter()` order; `None` for absent,
    /// binary or oversized content.
    pub texts: Vec<Option<Bytes>>,
    /// Text of the merge base, when known.
    pub base: Option<Bytes>,
}

/// A proposed resolution of one path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// The resolved content; `None` deletes the path.
    pub content: Option<(Bytes, FileKind)>,
    /// Why the resolver chose this, for the review thread and provenance.
    pub rationale: String,
    /// Confidence 0-100.
    pub confidence: u8,
}

/// An agent that resolves conflicts. Returning `None` means "I cannot
/// resolve this"; the pipeline then escalates to a human.
#[async_trait]
pub trait Resolver: Send + Sync {
    /// Stable name; resolutions carry `resolved_by = "agent"` provenance
    /// and this name.
    fn name(&self) -> &str;

    /// Propose a resolution for one conflicted path.
    async fn resolve(&self, conflict: &ConflictTerms) -> Result<Option<Resolution>>;
}
