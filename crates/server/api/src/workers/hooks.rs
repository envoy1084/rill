//! The lander's view of the review state machine.

use std::sync::Arc;

use arc_swap::ArcSwap;
use async_trait::async_trait;
use rill_lander::{Approval, ReviewHooks};
use rill_review::error::Error as ReviewError;
use rill_review::service::ReviewService;
use rill_types::{ChangeId, ChangeVersionHash, Principal, ReviewStatus, ThreadAnchor, TrunkSeq};

use crate::identity::AuthorStanding;

/// Adapts [`ReviewService`] to [`ReviewHooks`]. Holds the swappable
/// service handle so a policy reload is seen by in-flight landings.
pub struct ReviewHooksAdapter {
    review: Arc<ArcSwap<ReviewService>>,
    /// The repository's org and who judges its authors' standing.
    standing: Option<(String, Arc<dyn AuthorStanding>)>,
    /// Whether authors may still write what their versions change.
    rights: Option<super::rights::WriteRights>,
    /// For quarantined ancestors; without it only the version itself is
    /// checked.
    graph: Option<Arc<rill_core::CommitGraph>>,
}

impl ReviewHooksAdapter {
    /// Hooks over the given review service handle.
    pub fn new(review: Arc<ArcSwap<ReviewService>>) -> Self {
        ReviewHooksAdapter {
            review,
            standing: None,
            rights: None,
            graph: None,
        }
    }

    /// Also refuse landings whose author or requester may no longer act
    /// in `org`.
    pub fn with_standing(
        mut self,
        org: impl Into<String>,
        standing: Arc<dyn AuthorStanding>,
    ) -> Self {
        self.standing = Some((org.into(), standing));
        self
    }

    /// Also refuse landings whose author may no longer write what the
    /// version changes.
    /// Refuse versions with a quarantined ancestor off trunk.
    pub fn with_graph(mut self, graph: Arc<rill_core::CommitGraph>) -> Self {
        self.graph = Some(graph);
        self
    }

    pub fn with_write_rights(mut self, rights: super::rights::WriteRights) -> Self {
        self.rights = Some(rights);
        self
    }

    fn service(&self) -> Arc<ReviewService> {
        self.review.load_full()
    }
}

fn hook_err(e: ReviewError) -> rill_lander::Error {
    rill_lander::Error::Hook(e.to_string())
}

#[async_trait]
impl ReviewHooks for ReviewHooksAdapter {
    async fn mark_landing(&self, change: ChangeId) -> rill_lander::Result<()> {
        match self.service().mark_landing(&change).await {
            Ok(_) => Ok(()),
            // A change landed without going through review (a revert, or a
            // direct land in dev mode) has no review state to move.
            Err(ReviewError::ReviewNotFound(_)) => Ok(()),
            Err(e) => Err(hook_err(e)),
        }
    }

    async fn mark_landed(
        &self,
        change: ChangeId,
        seq: TrunkSeq,
        _version: ChangeVersionHash,
    ) -> rill_lander::Result<()> {
        self.service().announce_landed(&change, seq);
        Ok(())
    }

    async fn return_to_owner(&self, change: ChangeId, reason: String) -> rill_lander::Result<()> {
        let service = self.service();
        match service.landing_failed(&change).await {
            Ok(_) => {}
            Err(ReviewError::ReviewNotFound(_)) => return Ok(()),
            // Not in `Landing` (for example the land failed before the
            // review moved): just re-evaluate so the status is honest.
            Err(ReviewError::InvalidStatus { .. }) => {
                service.refresh(&change).await.map_err(hook_err)?;
            }
            Err(e) => return Err(hook_err(e)),
        }
        let state = service.get(&change).await.map_err(hook_err)?;
        let body = format!(
            "Landing was returned to the owner: {reason}\n\n\
             Rebase the change onto trunk, resolve any conflicts and request landing again."
        );
        service
            .comment(
                &change,
                &rill_lander::revert::lander_principal(),
                ThreadAnchor::Change,
                body,
                None,
                None,
                Some(state.owner.clone()),
            )
            .await
            .map_err(hook_err)?;
        Ok(())
    }

    async fn dependents_of(&self, change: ChangeId) -> rill_lander::Result<Vec<ChangeId>> {
        let stack = match self.service().stack_of(&change).await {
            Ok(stack) => stack,
            Err(ReviewError::ReviewNotFound(_)) => return Ok(Vec::new()),
            Err(e) => return Err(hook_err(e)),
        };
        Ok(stack
            .iter()
            .position(|c| *c == change)
            .map(|pos| stack[pos + 1..].to_vec())
            .unwrap_or_default())
    }

    /// Judge the review as stored now: still landing, still the version
    /// that was requested, and still allowed by the policy for the
    /// requester. The lander commits only against this very snapshot.
    async fn approve(
        &self,
        change: ChangeId,
        version: ChangeVersionHash,
        requester: &Principal,
    ) -> rill_lander::Result<Approval> {
        let service = self.service();
        let Some(review) = service.repo().read_review(&change).await? else {
            return Ok(Approval::default());
        };
        let state = &review.state;
        let refusal = if !matches!(state.status, ReviewStatus::Landing | ReviewStatus::Landable) {
            Some(format!("the change is {:?}", state.status))
        } else if state.current_version() != Some(version) {
            Some("a newer version was pushed; request landing again".to_string())
        } else {
            let decision = service
                .evaluate_for(state, requester)
                .await
                .map_err(hook_err)?;
            (!decision.allowed).then(|| {
                format!(
                    "the submit policy no longer allows landing: {}",
                    decision.reasons.join("; ")
                )
            })
        };
        let refusal = match (refusal, &self.standing) {
            (None, Some((org, standing))) => {
                match service.repo().get_change_version(&version).await? {
                    Some(cv) => {
                        standing
                            .refusal(org, &cv.author, &cv.provenance, requester)
                            .await
                    }
                    None => Some(format!("version {version} is missing")),
                }
            }
            (refusal, _) => refusal,
        };
        let refusal = match refusal {
            None => match &self.graph {
                Some(graph) => crate::services::repo::held_back(service.repo(), graph, &version)
                    .await
                    .map_err(|e| rill_lander::Error::Hook(e.to_string()))?,
                None => service.repo().quarantined(&version).await?,
            }
            .map(|why| {
                format!("it is quarantined by secret scanning ({why}); an owner must clear it")
            }),
            refusal => refusal,
        };
        let refusal = match (refusal, &self.rights) {
            (None, Some(rights)) => match service.repo().get_change_version(&version).await? {
                Some(cv) => rights.refusal(&cv).await,
                None => Some(format!("version {version} is missing")),
            },
            (refusal, _) => refusal,
        };
        let votes = state
            .votes
            .iter()
            .filter(|v| v.version == version)
            .cloned()
            .collect();
        Ok(Approval {
            policy: Some(service.policy().digest()),
            votes,
            refusal,
            review: Some(review),
        })
    }
}
