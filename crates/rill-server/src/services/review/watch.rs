//! `WatchReview`: one change's review events, from the event log, plus
//! land-ticket updates from the lander, which are not stored state.

use futures::StreamExt;
use rill_events::{EventFilter, Start};
use rill_lander::LandEvent;
use rill_proto::convert::{hash_to_proto, id_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, Event, EventKind, LabelVote, RepoPathBuf, ReviewStatus, Timestamp};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use super::{change_id, ReviewEventStream, ReviewSvc};
use crate::error::{Error, Result};
use crate::services::common::open;
use crate::state::{RepoContext, RepoEvent};

impl ReviewSvc {
    pub(super) async fn watch_review_impl(
        &self,
        request: tonic::Request<pb::WatchReviewRequest>,
    ) -> Result<ReviewEventStream> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let id = change_id(&request.get_ref().change_id)?;
        let ctx = opened.repo.clone();
        let mut bus = ctx.events.subscribe();
        let filter = EventFilter {
            change: Some(id),
            ..EventFilter::default()
        };
        let mut log = Box::pin(rill_events::tail(ctx.repo.clone(), Start::Now, filter).await?);
        let (tx, out) = mpsc::channel(256);
        tokio::spawn(async move {
            loop {
                let mapped = tokio::select! {
                    _ = ctx.shutdown.cancelled() => break,
                    next = log.next() => match next {
                        None => break,
                        Some(Ok((_, event))) => from_event(&ctx, &id, event).await,
                        Some(Err(e)) => {
                            let _ = tx.send(Err(Status::from(Error::from(e)))).await;
                            break;
                        }
                    },
                    received = bus.recv() => match received {
                        Ok(RepoEvent::Land(land)) => from_lander(&ctx, &id, &land),
                        Ok(_) | Err(RecvError::Lagged(_)) => None,
                        Err(RecvError::Closed) => break,
                    },
                };
                if let Some(mapped) = mapped {
                    if tx.send(Ok(mapped)).await.is_err() {
                        break;
                    }
                }
            }
        });
        Ok(Box::pin(ReceiverStream::new(out)))
    }
}

fn wrap(change_id: &ChangeId, at: Timestamp, event: pb::review_event::Event) -> pb::ReviewEvent {
    pb::ReviewEvent {
        change_id: id_to_proto(change_id),
        at: timestamp_to_proto(at),
        event: Some(event),
    }
}

/// The wire review event for a logged event about `change_id`.
async fn from_event(
    ctx: &RepoContext,
    change_id: &ChangeId,
    event: Event,
) -> Option<pb::ReviewEvent> {
    use pb::review_event::Event as E;
    let status = |s: ReviewStatus| E::StatusChanged(pb::ReviewStatus::from(s) as i32);
    let mapped = match event.kind {
        EventKind::ReviewSubmitted { .. } => status(ReviewStatus::Open),
        EventKind::StatusChanged { to, .. } => status(to),
        EventKind::Voted {
            label,
            value,
            version,
            ..
        } => E::Voted(pb::LabelVote::from(&LabelVote {
            label,
            value,
            voter: event.actor,
            version,
            voted_at: event.at,
            reason: None,
        })),
        EventKind::Commented { thread, .. } | EventKind::ThreadResolved { thread, .. } => {
            let state = ctx.review().get(change_id).await.ok()?;
            let thread = state.threads.iter().find(|t| t.id == thread)?;
            E::ThreadUpdated(pb::Thread::from(thread))
        }
        EventKind::VersionStored { version, .. } => E::NewVersion(hash_to_proto(&version)),
        EventKind::Landed { .. } => land_update(ctx, change_id, pb::LandStatus::Landed),
        _ => return None,
    };
    Some(wrap(change_id, event.at, mapped))
}

/// Ticket updates the lander reports; landings come from the log.
fn from_lander(
    ctx: &RepoContext,
    change_id: &ChangeId,
    land: &LandEvent,
) -> Option<pb::ReviewEvent> {
    let (ticket_change, status) = match land {
        LandEvent::Queued { change_id, .. } => (*change_id, pb::LandStatus::Queued),
        LandEvent::Conflict { change_id, .. } | LandEvent::Failed { change_id, .. } => {
            (*change_id, pb::LandStatus::Failed)
        }
        _ => return None,
    };
    (ticket_change == *change_id).then(|| {
        wrap(
            change_id,
            Timestamp::now(),
            land_update(ctx, change_id, status),
        )
    })
}

fn land_update(
    ctx: &RepoContext,
    change_id: &ChangeId,
    status: pb::LandStatus,
) -> pb::review_event::Event {
    let ticket = ctx
        .tickets
        .iter()
        .filter(|t| t.value().change_id == *change_id)
        .max_by_key(|t| *t.key())
        .map(|t| (*t.key(), t.value().requested_at));
    let (id, requested_at) = ticket.unwrap_or((0, Timestamp::now()));
    pb::review_event::Event::LandUpdate(pb::LandTicket {
        id: id.to_string(),
        change_id: id_to_proto(change_id),
        status: status as i32,
        requested_at: timestamp_to_proto(requested_at),
    })
}
