//! Intent budgets and links, kept from the event log: a change's first
//! version naming an intent links the change to it (whichever way it was
//! named) and counts against the day's `changes_per_day`; a landing of a
//! change under an intent counts against `lands_per_day`.

use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use rill_events::EventFilter;
use rill_policy::intents::{day_key, day_of, DayCounts};
use rill_types::{ChangeId, Event, EventKind, IntentId, IntentStatus, Timestamp};

use super::consumer::Consumer;
use crate::error::{Error, Result};
use crate::state::RepoContext;

/// Keeps budgets and links.
pub struct Budgets;

#[async_trait]
impl Consumer for Budgets {
    fn name(&self) -> &'static str {
        "intent-budgets"
    }

    fn filter(&self) -> EventFilter {
        EventFilter::kinds(["version_stored", "landed"])
    }

    async fn handle(&self, ctx: &Arc<RepoContext>, event: &Event) -> Result<()> {
        match &event.kind {
            EventKind::VersionStored { version, change } => {
                let cv = ctx.derive.change(version).await?;
                let Some(intent) = cv.intent else {
                    return Ok(());
                };
                if ctx.repo.change_versions(change).await?.len() > 1 {
                    return Ok(());
                }
                link(ctx, &intent, *change, event).await?;
                bump(ctx, &intent, event.at, |d| d.changes += 1).await
            }
            EventKind::Landed {
                change, version, ..
            } => {
                let review = ctx.repo.get_review(change).await?;
                let intent = ctx
                    .derive
                    .change(version)
                    .await?
                    .intent
                    .or(review.and_then(|r| r.intent));
                match intent {
                    Some(intent) => bump(ctx, &intent, event.at, |d| d.lands += 1).await,
                    None => Ok(()),
                }
            }
            _ => Ok(()),
        }
    }
}

/// Add `change` to the intent's list, as `LinkChange` would.
async fn link(ctx: &RepoContext, id: &IntentId, change: ChangeId, event: &Event) -> Result<()> {
    for _ in 0..rill_review::lifecycle::MAX_WRITE_ATTEMPTS {
        let Some(current) = ctx.repo.get_intent(id).await? else {
            return Ok(());
        };
        if current.changes.contains(&change) {
            return Ok(());
        }
        let mut next = current.clone();
        next.changes.push(change);
        next.updated_at = Timestamp::now();
        if next.status == IntentStatus::Open {
            next.status = IntentStatus::InProgress;
        }
        match ctx.repo.put_intent_if(&next, &current, &event.actor).await {
            Ok(()) => return Ok(()),
            Err(e) if e.is_conflict() => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Err(Error::precondition(format!("intent {id} kept changing")))
}

/// Apply `f` to the intent's counts for the day of `at`. Only this
/// consumer writes them, and one server runs it, so a plain write is safe.
async fn bump(
    ctx: &RepoContext,
    id: &IntentId,
    at: Timestamp,
    f: impl FnOnce(&mut DayCounts),
) -> Result<()> {
    let day = day_of(at);
    let mut counts = ctx.review().intent_day(id, &day).await?;
    f(&mut counts);
    let bytes = serde_json::to_vec(&counts).map_err(|e| Error::internal(e.to_string()))?;
    ctx.repo
        .set_misc(&day_key(id, &day), Bytes::from(bytes))
        .await?;
    Ok(())
}
