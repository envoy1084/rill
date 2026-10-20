//! Recording new versions of a change under review.

use rill_types::{ChangeId, ChangeVersionHash, LabelVote, ReviewState, ReviewStatus, Timestamp};

use super::interdiff::VersionRelation;
use super::retry::MAX_WRITE_ATTEMPTS;
use crate::error::{Error, Result};
use crate::service::{ReviewEvent, ReviewService};
use crate::votes::LabelSet;

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
        for _ in 0..MAX_WRITE_ATTEMPTS {
            let prev = self.get(change_id).await?;
            if prev.versions.contains(&new_version) {
                return Ok(prev);
            }
            if !matches!(prev.status, ReviewStatus::Open | ReviewStatus::Landable) {
                return Err(Error::InvalidStatus {
                    change_id: *change_id,
                    status: prev.status,
                    operation: "add_version",
                });
            }
            // Submit guarantees at least one version.
            let old_version = prev
                .current_version()
                .ok_or(Error::NoVersions(*change_id))?;
            let relation = self.relate_versions(&old_version, &new_version).await?;

            let mut next = prev.clone();
            next.versions.push(new_version);
            next.intent = cv.intent;
            let copied = copy_votes(&mut next, &self.labels, old_version, new_version, relation);
            self.reanchor(&mut next, new_version).await?;
            let facts = self.change_facts(&next).await?;
            self.apply_risk(&mut next, &facts);
            let transition = self.refresh_status(&mut next, &facts)?;
            next.updated_at = Timestamp::now().max(prev.updated_at);

            if self.try_put(&next, &prev).await? {
                tracing::info!(
                    change = %change_id,
                    version = %new_version,
                    ?relation,
                    votes_copied = copied,
                    "version added"
                );
                self.emit(ReviewEvent::VersionAdded {
                    change_id: *change_id,
                    version: new_version,
                    votes_copied: copied,
                });
                self.emit_transition(change_id, transition);
                return Ok(next);
            }
        }
        Err(Error::Contended(*change_id))
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
