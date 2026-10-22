//! Casting votes.

use rill_types::{
    ChangeId, LabelVote, Principal, PrincipalKind, ReviewState, ReviewStatus, Timestamp,
};

use crate::error::{Error, Result};
use crate::lifecycle::service_actor;
use crate::service::{ReviewEvent, ReviewService};
use crate::votes::LabelSet;

impl ReviewService {
    /// Cast `voter`'s vote on `label` for the change's current version,
    /// replacing any earlier vote by the same principal on the same label
    /// and version. The status is refreshed afterwards.
    pub async fn vote(
        &self,
        change_id: &ChangeId,
        voter: &Principal,
        label: &str,
        value: i8,
        reason: Option<String>,
    ) -> Result<ReviewState> {
        validate_vote(&self.labels, voter, label, value)?;
        let current = self.get(change_id).await?;
        let facts = self.change_facts(&current).await?;
        let (state, (vote, transition)) = self
            .update(change_id, voter, |s| {
                if !matches!(s.status, ReviewStatus::Open | ReviewStatus::Landable) {
                    return Err(Error::InvalidStatus {
                        change_id: *change_id,
                        status: s.status,
                        operation: "vote",
                    });
                }
                let version = s.current_version().ok_or(Error::NoVersions(*change_id))?;
                let vote = LabelVote {
                    label: label.to_string(),
                    value,
                    voter: voter.clone(),
                    version,
                    voted_at: Timestamp::now(),
                    reason: reason.clone(),
                };
                s.votes.retain(|v| {
                    !(v.voter.id == voter.id && v.label == label && v.version == version)
                });
                s.votes.push(vote.clone());
                let transition = self.refresh_status(s, &facts)?;
                Ok((vote, transition))
            })
            .await?;
        // Votes are written without touching the record, so a vote racing
        // this one may have been unseen when the status was computed.
        // Whoever writes last re-checks with every vote in view.
        let (state, transition) = match transition {
            Some(t) => (state, Some(t)),
            None => {
                self.update(change_id, &service_actor(), |s| {
                    self.refresh_status(s, &facts)
                })
                .await?
            }
        };
        tracing::info!(change = %change_id, voter = %voter, label, value, "vote cast");
        self.emit(ReviewEvent::Voted {
            change_id: *change_id,
            vote,
        });
        self.emit_transition(change_id, transition);
        Ok(state)
    }
}

/// Check a vote against the label configuration.
pub fn validate_vote(labels: &LabelSet, voter: &Principal, label: &str, value: i8) -> Result<()> {
    let def = labels
        .get(label)
        .ok_or_else(|| Error::UnknownLabel(label.to_string()))?;
    if value < def.range.min || value > def.range.max {
        return Err(Error::VoteOutOfRange {
            label: label.to_string(),
            value,
            min: def.range.min,
            max: def.range.max,
        });
    }
    if value == def.range.max && def.max_requires_human && voter.kind != PrincipalKind::Human {
        return Err(Error::MaxRequiresHuman(label.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rules() {
        let labels = LabelSet::default();
        let alice = Principal::human("alice");
        let bot = Principal::agent("bot");
        assert!(validate_vote(&labels, &alice, "Review", 2).is_ok());
        assert!(validate_vote(&labels, &bot, "Review", 2).is_ok());
        assert!(matches!(
            validate_vote(&labels, &bot, "Owner", 1),
            Err(Error::MaxRequiresHuman(_))
        ));
        assert!(validate_vote(&labels, &bot, "Owner", -1).is_ok());
        assert!(matches!(
            validate_vote(&labels, &alice, "Review", 3),
            Err(Error::VoteOutOfRange { .. })
        ));
        assert!(matches!(
            validate_vote(&labels, &alice, "Nope", 1),
            Err(Error::UnknownLabel(_))
        ));
    }
}
