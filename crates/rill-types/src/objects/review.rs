//! Review records: labels, votes, threads and the per-change review state.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::ids::{ChangeId, IntentId};
use crate::objects::change::ChangeVersionHash;
use crate::objects::intent::RiskClass;
use crate::objects::refs::TrunkSeq;
use crate::path::RepoPathBuf;
use crate::principal::Principal;
use crate::time::Timestamp;

/// A typed vote range on a change, e.g. `Review: -2..+2`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Label {
    pub name: String,
    pub range: LabelRange,
    /// Whether a vote survives a new version that is a trivial rebase or a
    /// message-only change.
    pub copy_on_trivial_rebase: bool,
    /// Principals (by kind) allowed to vote the maximum value.
    pub max_requires_human: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelRange {
    pub min: i8,
    pub max: i8,
}

impl Label {
    pub fn review() -> Self {
        Label {
            name: "Review".into(),
            range: LabelRange { min: -2, max: 2 },
            copy_on_trivial_rebase: true,
            max_requires_human: false,
        }
    }

    pub fn verified() -> Self {
        Label {
            name: "Verified".into(),
            range: LabelRange { min: -1, max: 1 },
            copy_on_trivial_rebase: false,
            max_requires_human: false,
        }
    }

    pub fn owner() -> Self {
        Label {
            name: "Owner".into(),
            range: LabelRange { min: -1, max: 1 },
            copy_on_trivial_rebase: true,
            max_requires_human: true,
        }
    }
}

/// One principal's vote on one label for one version.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelVote {
    pub label: String,
    pub value: i8,
    pub voter: Principal,
    pub version: ChangeVersionHash,
    pub voted_at: Timestamp,
    #[serde(default)]
    pub reason: Option<String>,
}

/// Where a thread is anchored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThreadAnchor {
    /// The change as a whole.
    Change,
    /// A line range in a file at a specific version. `context_hash` is a hash
    /// of the surrounding lines so the anchor can be re-found after rebases.
    Lines {
        version: ChangeVersionHash,
        path: RepoPathBuf,
        start: u32,
        end: u32,
        context_hash: String,
    },
    /// A whole file.
    File {
        version: ChangeVersionHash,
        path: RepoPathBuf,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommentCategory {
    Discussion,
    Bug,
    Security,
    Style,
    TestGap,
    Question,
    Suggestion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comment {
    pub author: Principal,
    pub body: String,
    pub created_at: Timestamp,
    #[serde(default)]
    pub category: Option<CommentCategory>,
    /// Reviewer confidence as a percentage 0-100; agents set it, humans
    /// usually do not.
    #[serde(default)]
    pub confidence: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thread {
    pub id: String,
    pub anchor: ThreadAnchor,
    pub comments: Vec<Comment>,
    pub resolved: bool,
    /// Who the thread is directed at, if anyone.
    #[serde(default)]
    pub to: Option<Principal>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReviewStatus {
    /// Work in progress; not yet submitted for review.
    Draft,
    Open,
    /// Submit rules pass; eligible for landing.
    Landable,
    /// Handed to the lander.
    Landing,
    Landed,
    Abandoned,
}

/// The review state of a change, keyed by its stable change id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewState {
    pub change_id: ChangeId,
    /// All versions in creation order; the last one is current.
    pub versions: Vec<ChangeVersionHash>,
    pub status: ReviewStatus,
    pub intent: Option<IntentId>,
    pub owner: Principal,
    pub votes: Vec<LabelVote>,
    pub threads: Vec<Thread>,
    /// Risk class of the current version as computed by policy.
    pub risk: RiskClass,
    /// Explanation of the risk classification: feature names that fired.
    #[serde(default)]
    pub risk_reasons: Vec<String>,
    /// Lands as part of this stack (change ids in bottom-to-top order).
    #[serde(default)]
    pub stack: Vec<ChangeId>,
    pub landed_seq: Option<TrunkSeq>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

impl ReviewState {
    pub fn current_version(&self) -> Option<ChangeVersionHash> {
        self.versions.last().copied()
    }

    /// Votes that apply to the current version.
    pub fn current_votes(&self) -> impl Iterator<Item = &LabelVote> {
        let current = self.current_version();
        self.votes
            .iter()
            .filter(move |v| Some(v.version) == current)
    }

    pub fn unresolved_threads(&self) -> usize {
        self.threads.iter().filter(|t| !t.resolved).count()
    }
}
