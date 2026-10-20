//! Entering review.

use std::collections::BTreeMap;

use rill_types::{ChangeId, Principal, ReviewState, ReviewStatus, RiskClass, Timestamp};

use crate::error::{Error, Result};
use crate::service::{ReviewEvent, ReviewService};

impl ReviewService {
    /// Put a change under review. The review starts `Open` with every
    /// stored version of the change, the last one current, and a risk class
    /// computed for that version.
    ///
    /// `author` becomes the review's owner: the principal whose `Owner`
    /// vote (or sponsor's) the policy looks for.
    pub async fn submit(&self, change_id: &ChangeId, author: &Principal) -> Result<ReviewState> {
        let versions = self.repo.change_versions(change_id).await?;
        let Some(&current) = versions.last() else {
            return Err(Error::NoVersions(*change_id));
        };
        if self.repo.get_review(change_id).await?.is_some() {
            return Err(Error::AlreadySubmitted(*change_id));
        }
        let cv = self.derive.change(&current).await?;
        let now = Timestamp::now();
        let mut state = ReviewState {
            change_id: *change_id,
            versions,
            status: ReviewStatus::Open,
            intent: cv.intent,
            owner: author.clone(),
            votes: Vec::new(),
            threads: Vec::new(),
            risk: RiskClass::Normal,
            risk_reasons: Vec::new(),
            stack: Vec::new(),
            landed_seq: None,
            created_at: now,
            updated_at: now,
            extra: BTreeMap::new(),
        };
        let facts = self.change_facts(&state).await?;
        self.apply_risk(&mut state, &facts);
        let transition = self.refresh_status(&mut state, &facts)?;
        self.repo.put_review(&state, None).await?;
        tracing::info!(change = %change_id, risk = %state.risk, "change submitted for review");
        self.emit(ReviewEvent::Submitted {
            change_id: *change_id,
            version: current,
            author: author.clone(),
        });
        self.emit_transition(change_id, transition);
        Ok(state)
    }

    /// Publish a status change, if any.
    pub(crate) fn emit_transition(
        &self,
        change_id: &ChangeId,
        transition: Option<(ReviewStatus, ReviewStatus)>,
    ) {
        if let Some((from, to)) = transition {
            self.emit(ReviewEvent::StatusChanged {
                change_id: *change_id,
                from,
                to,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixture;

    #[tokio::test]
    async fn submit_records_versions_and_risk() {
        let fx = fixture().await;
        let (_, base) = fx.root_change("src/lib.rs", "fn a() {}\n").await;
        let (cid, v1) = fx
            .child_change(base, "src/lib.rs", "fn a() {}\nfn b() {}\n")
            .await;
        let mut rx = fx.service.subscribe();
        let state = fx.service.submit(&cid, &fx.alice).await.unwrap();
        assert_eq!(state.versions, vec![v1]);
        assert_eq!(state.status, ReviewStatus::Open);
        assert_eq!(state.risk, RiskClass::Normal);
        assert_eq!(state.owner, fx.alice);
        assert!(matches!(
            rx.try_recv().unwrap(),
            ReviewEvent::Submitted { change_id, .. } if change_id == cid
        ));
        assert!(matches!(
            fx.service.submit(&cid, &fx.alice).await,
            Err(Error::AlreadySubmitted(_))
        ));
        assert!(matches!(
            fx.service.submit(&ChangeId::new(), &fx.alice).await,
            Err(Error::NoVersions(_))
        ));
    }

    #[tokio::test]
    async fn docs_only_change_is_trivial() {
        let fx = fixture().await;
        let (_, base) = fx.root_change("README.md", "# hi\n").await;
        let (cid, _) = fx.child_change(base, "README.md", "# hello\n").await;
        let state = fx.service.submit(&cid, &fx.alice).await.unwrap();
        assert_eq!(state.risk, RiskClass::Trivial);
        assert_eq!(state.risk_reasons, vec!["docs or comments only"]);
    }
}
