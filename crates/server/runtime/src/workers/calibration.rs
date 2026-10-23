//! Calibrating reviewers from what happens after review.
//!
//! A landing credits every agent whose positive `Review` vote the policy
//! saw. A revert of that landing, or an incident blaming it, turns those
//! credits into escapes, once per landing whichever comes first (a failed
//! audit too). A
//! blocking finding (Bug or Security, confidence 70 or more) resolved
//! after the author stored a newer version counts as a catch; resolved on
//! the version it was raised on, as a false alarm. A reviewer whose trust
//! falls too low is demoted.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_events::EventFilter;
use rill_lander::revert::REVERTS_KEY;
use rill_policy::submit::REVIEW_LABEL;
use rill_types::objects::review::CommentCategory;
use rill_types::{Event, EventKind, PrincipalKind, ThreadAnchor, Timestamp, TrunkSeq};

use super::consumer::Consumer;
use crate::error::Result;
use crate::state::RepoContext;

/// A finding at or above this confidence blocks.
const BLOCKING_CONFIDENCE: u8 = 70;

/// Keeps reviewers' records.
pub struct Calibrator;

#[async_trait]
impl Consumer for Calibrator {
    fn name(&self) -> &'static str {
        "reviewer-calibration"
    }

    fn filter(&self) -> EventFilter {
        EventFilter::kinds(["landed", "incident_changed", "thread_resolved"])
    }

    async fn handle(&self, ctx: &Arc<RepoContext>, event: &Event) -> Result<()> {
        match &event.kind {
            EventKind::Landed { seq, version, .. } => {
                let cv = ctx.derive.change(version).await?;
                match cv
                    .provenance
                    .extra
                    .get(REVERTS_KEY)
                    .and_then(|s| s.parse().ok())
                {
                    Some(reverted) => escape(ctx, TrunkSeq(reverted)).await,
                    None => credit(ctx, *seq).await,
                }
            }
            EventKind::IncidentChanged { culprit, state, .. } if state == "open" => {
                escape(ctx, *culprit).await
            }
            EventKind::ThreadResolved {
                change,
                thread,
                resolved: true,
            } => finding_outcome(ctx, change, thread).await,
            _ => Ok(()),
        }
    }
}

/// The agents whose positive `Review` votes the landing at `seq` had.
async fn approvers(ctx: &RepoContext, seq: TrunkSeq) -> Result<BTreeSet<String>> {
    let Some(landing) = ctx.repo.landing(seq).await? else {
        return Ok(BTreeSet::new());
    };
    Ok(landing
        .approvals
        .iter()
        .filter(|v| v.label == REVIEW_LABEL && v.value > 0 && v.voter.kind == PrincipalKind::Agent)
        .map(|v| v.voter.name.clone())
        .collect())
}

async fn credit(ctx: &RepoContext, seq: TrunkSeq) -> Result<()> {
    let agents = approvers(ctx, seq).await?;
    if agents.is_empty() {
        return Ok(());
    }
    ctx.review()
        .update_reviewer_stats(|map| {
            for a in &agents {
                map.entry(a.clone()).or_default().approvals += 1;
            }
        })
        .await?;
    Ok(())
}

/// Count the landing at `seq` as an escape for its approvers, once.
pub async fn escape(ctx: &RepoContext, seq: TrunkSeq) -> Result<()> {
    let marker = rill_review::attention::escaped_key(seq);
    match ctx
        .repo
        .set_misc_cas(&marker, None, Bytes::from_static(b"1"))
        .await
    {
        Ok(()) => {}
        Err(e) if e.is_conflict() => return Ok(()),
        Err(e) => return Err(e.into()),
    }
    let agents = approvers(ctx, seq).await?;
    if agents.is_empty() {
        return Ok(());
    }
    let now = Timestamp::now();
    let demoted = ctx
        .review()
        .update_reviewer_stats(|map| {
            let mut demoted = Vec::new();
            for a in &agents {
                let s = map.entry(a.clone()).or_default();
                s.escapes += 1;
                if s.check(now) {
                    demoted.push(a.clone());
                }
            }
            demoted
        })
        .await?;
    for a in demoted {
        tracing::warn!(repo = %ctx.config.name, reviewer = %a, "reviewer demoted");
    }
    Ok(())
}

async fn finding_outcome(
    ctx: &RepoContext,
    change: &rill_types::ChangeId,
    thread: &str,
) -> Result<()> {
    let Some(state) = ctx.repo.get_review(change).await? else {
        return Ok(());
    };
    let Some(t) = state.threads.iter().find(|t| t.id == thread) else {
        return Ok(());
    };
    let Some(first) = t.comments.first() else {
        return Ok(());
    };
    let blocking = first.author.kind == PrincipalKind::Agent
        && matches!(
            first.category,
            Some(CommentCategory::Bug | CommentCategory::Security)
        )
        && first.confidence.unwrap_or(0) >= BLOCKING_CONFIDENCE;
    let raised_on = match &t.anchor {
        ThreadAnchor::File { version, .. } | ThreadAnchor::Lines { version, .. } => *version,
        ThreadAnchor::Change => return Ok(()),
    };
    if !blocking {
        return Ok(());
    }
    let fixed = state.current_version() != Some(raised_on);
    let agent = first.author.name.clone();
    let now = Timestamp::now();
    let demoted = ctx
        .review()
        .update_reviewer_stats(|map| {
            let s = map.entry(agent.clone()).or_default();
            if fixed {
                s.blocks += 1;
            } else {
                s.dismissed += 1;
            }
            s.check(now)
        })
        .await?;
    if demoted {
        tracing::warn!(repo = %ctx.config.name, reviewer = %agent, "reviewer demoted");
    }
    Ok(())
}
