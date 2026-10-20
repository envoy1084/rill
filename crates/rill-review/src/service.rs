//! The review service: shared handles plus the event stream.

use std::sync::Arc;

use rill_core::{CommitGraph, DeriveManager};
use rill_policy::{CompiledRiskConfig, SubmitPolicy};
use rill_store::RepoStore;
use rill_types::{
    ChangeId, ChangeVersionHash, LabelVote, Principal, ReviewState, ReviewStatus, TrunkSeq,
};
use tokio::sync::broadcast;

use crate::error::{Error, Result};
use crate::votes::LabelSet;

/// Default capacity of the event channel.
pub const DEFAULT_EVENT_CAPACITY: usize = 1024;

/// Something that happened to a review, published to subscribers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewEvent {
    /// A change entered review.
    Submitted {
        change_id: ChangeId,
        version: ChangeVersionHash,
        author: Principal,
    },
    /// A new version of the change was recorded.
    VersionAdded {
        change_id: ChangeId,
        version: ChangeVersionHash,
        /// Whether votes were carried over from the previous version.
        votes_copied: bool,
    },
    /// A vote was cast or replaced.
    Voted {
        change_id: ChangeId,
        vote: LabelVote,
    },
    /// A comment was posted (new thread or reply).
    Commented {
        change_id: ChangeId,
        thread_id: String,
        author: Principal,
        /// True when this comment opened the thread.
        new_thread: bool,
    },
    /// A thread was resolved.
    ThreadResolved {
        change_id: ChangeId,
        thread_id: String,
        by: Principal,
    },
    /// The change's stack membership changed.
    StackChanged {
        change_id: ChangeId,
        stack: Vec<ChangeId>,
    },
    /// The status moved.
    StatusChanged {
        change_id: ChangeId,
        from: ReviewStatus,
        to: ReviewStatus,
    },
    /// The change landed on trunk.
    Landed { change_id: ChangeId, seq: TrunkSeq },
    /// The change was abandoned.
    Abandoned { change_id: ChangeId, by: Principal },
}

impl ReviewEvent {
    /// The change the event concerns.
    pub fn change_id(&self) -> ChangeId {
        match self {
            ReviewEvent::Submitted { change_id, .. }
            | ReviewEvent::VersionAdded { change_id, .. }
            | ReviewEvent::Voted { change_id, .. }
            | ReviewEvent::Commented { change_id, .. }
            | ReviewEvent::ThreadResolved { change_id, .. }
            | ReviewEvent::StackChanged { change_id, .. }
            | ReviewEvent::StatusChanged { change_id, .. }
            | ReviewEvent::Landed { change_id, .. }
            | ReviewEvent::Abandoned { change_id, .. } => *change_id,
        }
    }
}

/// The review state machine for one repository.
///
/// All mutation goes through optimistic writes of the whole
/// [`ReviewState`] record, so the service holds no state of its own beyond
/// the shared handles and can be cloned freely behind an `Arc`.
pub struct ReviewService {
    pub(crate) repo: RepoStore,
    pub(crate) derive: Arc<DeriveManager>,
    pub(crate) graph: Arc<CommitGraph>,
    pub(crate) policy: Arc<SubmitPolicy>,
    pub(crate) risk: Arc<CompiledRiskConfig>,
    pub(crate) labels: Arc<LabelSet>,
    pub(crate) events: broadcast::Sender<ReviewEvent>,
}

impl ReviewService {
    /// Create a service over `repo` with the given policy inputs. The event
    /// channel is created with [`DEFAULT_EVENT_CAPACITY`].
    pub fn new(
        repo: RepoStore,
        derive: Arc<DeriveManager>,
        graph: Arc<CommitGraph>,
        policy: Arc<SubmitPolicy>,
        risk: Arc<CompiledRiskConfig>,
        labels: Arc<LabelSet>,
    ) -> Self {
        let (events, _) = broadcast::channel(DEFAULT_EVENT_CAPACITY);
        ReviewService {
            repo,
            derive,
            graph,
            policy,
            risk,
            labels,
            events,
        }
    }

    /// Use an existing event channel, for sharing one bus across services.
    pub fn with_events(mut self, events: broadcast::Sender<ReviewEvent>) -> Self {
        self.events = events;
        self
    }

    /// Subscribe to review events. Slow subscribers lag rather than block.
    pub fn subscribe(&self) -> broadcast::Receiver<ReviewEvent> {
        self.events.subscribe()
    }

    /// The repository store.
    pub fn repo(&self) -> &RepoStore {
        &self.repo
    }

    /// The derived-data manager.
    pub fn derive(&self) -> &Arc<DeriveManager> {
        &self.derive
    }

    /// The change graph.
    pub fn graph(&self) -> &Arc<CommitGraph> {
        &self.graph
    }

    /// The submit policy.
    pub fn policy(&self) -> &Arc<SubmitPolicy> {
        &self.policy
    }

    /// The compiled risk configuration.
    pub fn risk_config(&self) -> &Arc<CompiledRiskConfig> {
        &self.risk
    }

    /// The configured labels.
    pub fn labels(&self) -> &Arc<LabelSet> {
        &self.labels
    }

    /// The review state of a change.
    pub async fn get(&self, change_id: &ChangeId) -> Result<ReviewState> {
        self.repo
            .get_review(change_id)
            .await?
            .ok_or(Error::ReviewNotFound(*change_id))
    }

    /// All review states in the repository.
    pub async fn list(&self) -> Result<Vec<ReviewState>> {
        Ok(self.repo.list_reviews().await?)
    }

    /// Publish an event; a missing subscriber is not an error.
    pub(crate) fn emit(&self, event: ReviewEvent) {
        let _ = self.events.send(event);
    }
}
