//! Recording new versions of a change under review.

use rill_store::repo::ReviewSnapshot;
use rill_types::{ChangeId, ChangeVersionHash, LabelVote, ReviewState, ReviewStatus, Timestamp};

use super::interdiff::VersionRelation;
use super::retry::MAX_WRITE_ATTEMPTS;
use crate::error::{Error, Result};
use crate::service::{ReviewEvent, ReviewService};
use crate::votes::LabelSet;

/// A new version's effect on a review, computed but not yet written.
#[derive(Debug, Clone)]
pub struct PreparedVersion {
    /// The review as read; the write checks against it.
    pub prev: ReviewSnapshot,
    /// The review with the version added.
    pub next: ReviewState,
    pub version: ChangeVersionHash,
    pub relation: VersionRelation,
    pub votes_copied: bool,
    pub transition: Option<(ReviewStatus, ReviewStatus)>,
}

impl ReviewService {
    /// Append `new_version` as the current version of the change.
    ///
    /// Risk is recomputed, line-anchored threads are re-anchored into the
    /// new content, and votes are copied from the previous version when the
    /// new one is a message-only change or a trivial rebase (for labels
    /// that allow it); otherwise votes stay attached to the old version and
    /// the change must be re-approved. Recording a version that is already
    /// current is a no-op.
    pub async fn add_version(
        &self,
        change_id: &ChangeId,
        new_version: ChangeVersionHash,
    ) -> Result<ReviewState> {
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let Some(prepared) = self.prepare_version(change_id, new_version).await? else {
                return self.get(change_id).await;
            };
            match self.repo.write_review(&prepared.prev, &prepared.next).await {
                Ok(()) => {
                    let state = prepared.next.clone();
                    self.version_written(prepared);
                    return Ok(state);
                }
                Err(e) if e.is_conflict() => continue,
                Err(e) => return Err(e.into()),
            }
        }
        Err(Error::Contended(*change_id))
    }

    /// What adding `new_version` would write, or `None` when it is already
    /// recorded. Fails for a version of another change or a review that no
    /// longer takes versions.
    pub async fn prepare_version(
        &self,
        change_id: &ChangeId,
        new_version: ChangeVersionHash,
    ) -> Result<Option<PreparedVersion>> {
        let cv = self
            .repo
            .get_change_version(&new_version)
            .await?
            .ok_or(Error::VersionNotFound(new_version))?;
        if cv.change_id != *change_id {
            return Err(Error::WrongChange {
                version: new_version,
                expected: *change_id,
                actual: cv.change_id,
            });
        }
        let prev = self
            .repo
            .read_review(change_id)
            .await?
            .ok_or(Error::ReviewNotFound(*change_id))?;
        if prev.state.versions.contains(&new_version) {
            return Ok(None);
        }
        if !matches!(
            prev.state.status,
            ReviewStatus::Open | ReviewStatus::Landable
        ) {
            return Err(Error::InvalidStatus {
                change_id: *change_id,
                status: prev.state.status,
                operation: "add_version",
            });
        }
        // Submit guarantees at least one version.
        let old_version = prev
            .state
            .current_version()
            .ok_or(Error::NoVersions(*change_id))?;
        let relation = self.relate_versions(&old_version, &new_version).await?;

        let mut next = prev.state.clone();
        next.versions.push(new_version);
        next.intent = cv.intent;
        let votes_copied = copy_votes(&mut next, &self.labels, old_version, new_version, relation);
        self.reanchor(&mut next, new_version).await?;
        let facts = self.change_facts(&next).await?;
        self.apply_risk(&mut next, &facts);
        let transition = self.refresh_status(&mut next, &facts)?;
        next.updated_at = Timestamp::now().max(prev.state.updated_at);
        Ok(Some(PreparedVersion {
            prev,
            next,
            version: new_version,
            relation,
            votes_copied,
            transition,
        }))
    }

    /// Announce a version whose review update was written.
    pub fn version_written(&self, prepared: PreparedVersion) {
        let change_id = prepared.next.change_id;
        tracing::info!(
            change = %change_id,
            version = %prepared.version,
            relation = ?prepared.relation,
            votes_copied = prepared.votes_copied,
            "version added"
        );
        self.emit(ReviewEvent::VersionAdded {
            change_id,
            version: prepared.version,
            votes_copied: prepared.votes_copied,
        });
        self.emit_transition(&change_id, prepared.transition);
    }
}

/// Copy the previous version's votes onto the new one when the relation
/// allows it. Returns whether any vote was copied.
fn copy_votes(
    state: &mut ReviewState,
    labels: &LabelSet,
    old_version: ChangeVersionHash,
    new_version: ChangeVersionHash,
    relation: VersionRelation,
) -> bool {
    if !relation.preserves_votes() {
        return false;
    }
    let copied: Vec<LabelVote> = state
        .votes
        .iter()
        .filter(|v| v.version == old_version)
        .filter(|v| {
            labels
                .get(&v.label)
                .is_some_and(|l| l.copy_on_trivial_rebase)
        })
        .map(|v| LabelVote {
            version: new_version,
            ..v.clone()
        })
        .collect();
    let any = !copied.is_empty();
    state.votes.extend(copied);
    any
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fixture;

    #[tokio::test]
    async fn rejects_foreign_or_missing_versions() {
        let fx = fixture().await;
        let (_, base) = fx.root_change("a.txt", "1\n").await;
        let (cid, _) = fx.child_change(base, "b.txt", "2\n").await;
        let (other, other_v) = fx.child_change(base, "c.txt", "3\n").await;
        fx.service.submit(&cid, &fx.alice).await.unwrap();
        assert!(matches!(
            fx.service.add_version(&cid, other_v).await,
            Err(Error::WrongChange { actual, .. }) if actual == other
        ));
        assert!(matches!(
            fx.service
                .add_version(&cid, rill_types::ObjectHash::ZERO)
                .await,
            Err(Error::VersionNotFound(_))
        ));
    }

    #[tokio::test]
    async fn adding_current_version_is_noop() {
        let fx = fixture().await;
        let (_, base) = fx.root_change("a.txt", "1\n").await;
        let (cid, v1) = fx.child_change(base, "b.txt", "2\n").await;
        let s1 = fx.service.submit(&cid, &fx.alice).await.unwrap();
        let s2 = fx.service.add_version(&cid, v1).await.unwrap();
        assert_eq!(s1, s2);
    }
}
