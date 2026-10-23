//! What runs a job: a deterministic fake, or a model behind the Claude
//! Messages API.

mod claude;
mod claude_tools;
mod fake;

use async_trait::async_trait;
use rill_sdk::{ReviewJob, ReviewReport};

pub use claude::Claude;
pub use fake::{Fake, DEFECT_MARKER};

use crate::error::Result;

/// One conflicted file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictFile {
    pub path: String,
    /// The merge base's text, when there is one and it is text.
    pub base: Option<String>,
    /// Each side's text; `None` for an absent or binary side.
    pub sides: Vec<Option<String>>,
}

/// A conflicted version to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveTask {
    /// The resolver's prompt from the repository, when it has one.
    pub prompt: String,
    pub files: Vec<ConflictFile>,
}

/// What a resolver decided.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveOutcome {
    /// New content per path; `None` deletes it.
    Resolved {
        files: Vec<(String, Option<String>)>,
        rationale: String,
        model: String,
    },
    /// The resolver will not resolve it; a human will.
    Declined(String),
}

/// A backend reviews and resolves.
#[async_trait]
pub trait Backend: Send + Sync {
    async fn review(&self, job: &ReviewJob) -> Result<ReviewReport>;
    async fn resolve(&self, task: &ResolveTask) -> Result<ResolveOutcome>;
}

/// The vote a set of findings earns: against when one should block, for
/// otherwise; within the reviewer's maximum.
pub fn vote_for(findings: &[rill_sdk::ReviewFinding], max_vote: i8) -> i8 {
    use rill_types::objects::review::CommentCategory::{Bug, Security};
    let blocks = findings
        .iter()
        .any(|f| matches!(f.category, Bug | Security) && f.confidence >= 70);
    let one = 1.min(max_vote.abs());
    if blocks {
        -one
    } else {
        one
    }
}
