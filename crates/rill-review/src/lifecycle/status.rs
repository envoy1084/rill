//! Status transitions driven by people and by the lander.

use rill_types::{ChangeId, Principal, ReviewState, ReviewStatus, TrunkSeq};

use crate::error::{Error, Result};
use crate::service::{ReviewEvent, ReviewService};

impl ReviewService {
    /// Abandon an unlanded change.
    pub async fn abandon(&self, change_id: &ChangeId, by: &Principal) -> Result<ReviewState> {
        let state = self
            .transition(
                change_id,
                &[ReviewStatus::Open, ReviewStatus::Landable],
                ReviewStatus::Abandoned,
                "abandon",
            )
            .await?;
        self.emit(ReviewEvent::Abandoned {
            change_id: *change_id,
            by: by.clone(),
        });
        Ok(state)
    }

    /// Reopen an abandoned change. The status is re-evaluated against the
    /// submit policy, so it may come back as `Landable`.
    pub async fn reopen(&self, change_id: &ChangeId) -> Result<ReviewState> {
        self.transition(
            change_id,
            &[ReviewStatus::Abandoned],
            ReviewStatus::Open,
            "reopen",
        )
        .await?;
        self.refresh(change_id).await
    }

    /// Hand a landable change to the lander. Fails unless the change is
    /// currently `Landable`, which is the lander's guard against landing a
    /// change whose approvals were withdrawn.
    pub async fn mark_landing(&self, change_id: &ChangeId) -> Result<ReviewState> {
        self.transition(
            change_id,
            &[ReviewStatus::Landable],
            ReviewStatus::Landing,
            "mark_landing",
        )
        .await
    }

    /// Announce that the change landed at `seq`. The land transaction
    /// already marked the review `Landed`; this publishes the events.
    pub fn announce_landed(&self, change_id: &ChangeId, seq: TrunkSeq) {
        self.emit_transition(
            change_id,
            Some((ReviewStatus::Landing, ReviewStatus::Landed)),
        );
        self.emit(ReviewEvent::Landed {
            change_id: *change_id,
            seq,
        });
    }

    /// Return a change the lander gave up on to the review queue.
    pub async fn landing_failed(&self, change_id: &ChangeId) -> Result<ReviewState> {
        self.transition(
            change_id,
            &[ReviewStatus::Landing],
            ReviewStatus::Open,
            "landing_failed",
        )
        .await?;
        self.refresh(change_id).await
    }

    /// Move the status to `to` if it is currently one of `from`.
    async fn transition(
        &self,
        change_id: &ChangeId,
        from: &[ReviewStatus],
        to: ReviewStatus,
        operation: &'static str,
    ) -> Result<ReviewState> {
        let (state, transition) = self
            .update(change_id, |s| {
                if !from.contains(&s.status) {
                    return Err(Error::InvalidStatus {
                        change_id: *change_id,
                        status: s.status,
                        operation,
                    });
                }
                let before = s.status;
                s.status = to;
                Ok((before, to))
            })
            .await?;
        self.emit_transition(change_id, Some(transition));
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixture;

    #[tokio::test]
    async fn lifecycle_transitions() {
        let fx = fixture().await;
        let (_, base) = fx.root_change("a.txt", "1\n").await;
        let (cid, _) = fx.child_change(base, "b.rs", "fn b() {}\n").await;
        fx.service.submit(&cid, &fx.alice).await.unwrap();

        // Not landable yet: no approvals.
        assert!(matches!(
            fx.service.mark_landing(&cid).await,
            Err(Error::InvalidStatus {
                status: ReviewStatus::Open,
                ..
            })
        ));

        let s = fx.service.abandon(&cid, &fx.alice).await.unwrap();
        assert_eq!(s.status, ReviewStatus::Abandoned);
        // Re-adding the current version is idempotent even when abandoned.
        assert!(fx.service.add_version(&cid, s.versions[0]).await.is_ok());
        let s = fx.service.reopen(&cid).await.unwrap();
        assert_eq!(s.status, ReviewStatus::Open);

        fx.service
            .vote(&cid, &fx.alice, "Review", 2, None)
            .await
            .unwrap();
        let s = fx.service.mark_landing(&cid).await.unwrap();
        assert_eq!(s.status, ReviewStatus::Landing);
        let s = fx.service.landing_failed(&cid).await.unwrap();
        assert_eq!(s.status, ReviewStatus::Landable);
        let landing = fx.service.mark_landing(&cid).await.unwrap();
        // The land transaction marks the review landed.
        let version = landing.current_version().unwrap();
        let repo = fx.service.repo();
        let head = repo.trunk_head().await.unwrap().map(|(_, h)| h);
        repo.land(rill_store::repo::LandBatch {
            expected: head,
            entries: vec![rill_store::repo::LandEntry {
                version,
                change_id: cid,
                requested_version: version,
                requester: fx.alice.clone(),
                risk: rill_types::RiskClass::Normal,
                review: repo.read_review(&cid).await.unwrap(),
            }],
            ..Default::default()
        })
        .await
        .unwrap();
        let s = fx.service.get(&cid).await.unwrap();
        assert_eq!(s.status, ReviewStatus::Landed);
        assert!(s.landed_seq.is_some());
        assert!(matches!(
            fx.service.abandon(&cid, &fx.alice).await,
            Err(Error::InvalidStatus { .. })
        ));
    }
}
