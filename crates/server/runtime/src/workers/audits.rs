//! Sampling landings that no human approved for a human audit.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use rill_events::EventFilter;
use rill_lander::revert::REVERTS_KEY;
use rill_policy::submit::{OWNER_LABEL, REVIEW_LABEL};
use rill_review::attention::{sampled, Audit, AuditStatus, DEFAULT_SAMPLE_PERMILLE};
use rill_types::{Event, EventKind, PrincipalKind, Timestamp};

use super::consumer::Consumer;
use crate::error::Result;
use crate::state::RepoContext;

/// Opens audits.
pub struct Sampler;

#[async_trait]
impl Consumer for Sampler {
    fn name(&self) -> &'static str {
        "audit-sampler"
    }

    fn filter(&self) -> EventFilter {
        EventFilter::kinds(["landed"])
    }

    async fn handle(&self, ctx: &Arc<RepoContext>, event: &Event) -> Result<()> {
        let EventKind::Landed {
            seq,
            change,
            version,
        } = &event.kind
        else {
            return Ok(());
        };
        let Some(landing) = ctx.repo.landing(*seq).await? else {
            return Ok(());
        };
        let cv = ctx.derive.change(version).await?;
        if landing.policy.is_none() || cv.provenance.extra.contains_key(REVERTS_KEY) {
            return Ok(());
        }
        let human_approved = landing.approvals.iter().any(|v| {
            (v.label == REVIEW_LABEL || v.label == OWNER_LABEL)
                && v.value > 0
                && v.voter.kind == PrincipalKind::Human
        });
        if human_approved {
            return Ok(());
        }
        let review = ctx.review();
        let state = ctx.repo.get_review(change).await?;
        let intent = match cv.intent.or(state.as_ref().and_then(|s| s.intent)) {
            Some(id) => ctx.repo.get_intent(&id).await?,
            None => None,
        };
        let permille = intent
            .as_ref()
            .and_then(|i| i.scope.as_ref())
            .and_then(|s| s.human_sample_permille)
            .unwrap_or(DEFAULT_SAMPLE_PERMILLE);
        if !sampled(*seq, permille, landing.risk) {
            return Ok(());
        }
        let owners = review.trunk_owners().await?;
        let paths = review.touched_paths(version).await?;
        let mut assignees: BTreeSet<String> = owners
            .sets_for_all(paths.iter().map(|p| p.as_path()))
            .into_iter()
            .flat_map(|s| s.owners)
            .collect();
        if assignees.is_empty() {
            assignees.insert("owners".into());
        }
        let opened = review
            .open_audit(&Audit {
                seq: *seq,
                change: *change,
                version: *version,
                assignees: assignees.into_iter().collect(),
                risk: landing.risk,
                status: AuditStatus::Pending,
                sampled_at: Timestamp::now(),
                decided_by: None,
                decided_at: None,
                note: String::new(),
            })
            .await?;
        if opened {
            tracing::info!(repo = %ctx.config.name, seq = seq.0, "landing sampled for audit");
        }
        Ok(())
    }
}
