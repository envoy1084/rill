//! `WatchReview`: one change's review and land-request events, from the
//! event log.

use rill_events::{EventFilter, Start};
use rill_proto::convert::{hash_to_proto, id_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, Event, EventKind, LabelVote, RepoPathBuf, ReviewStatus, Timestamp};

use super::reviews::change_id;
use super::ReviewSvc;
use crate::services::common::{self, open};
use crate::services::repo::forward;
use rill_runtime::error::Result;
use rill_runtime::state::RepoContext;

/// Server stream of review events.
pub type ReviewEventStream = crate::services::repo::EventStream<pb::ReviewEvent>;

impl ReviewSvc {
    pub(super) async fn watch_review_impl(
        &self,
        request: tonic::Request<pb::WatchReviewRequest>,
    ) -> Result<ReviewEventStream> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let id = change_id(&request.get_ref().change_id)?;
        let ctx = opened.repo.clone();
        let filter = EventFilter {
            change: Some(id),
            ..EventFilter::default()
        };
        let log = rill_events::tail(ctx.repo.clone(), Start::Now, filter).await?;
        let reader = ctx.clone();
        Ok(forward(ctx, log, move |_, event| {
            let ctx = reader.clone();
            async move { Ok(from_event(&ctx, &id, event).await) }
        }))
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
        EventKind::LandRequested { ticket, .. } => {
            let record = ctx.lander.queue().record(ticket).await.ok()??;
            E::LandUpdate(common::ticket_proto(&rill_types::LandRequestRecord {
                status: rill_types::LandStatus::Queued,
                ..record
            }))
        }
        EventKind::LandStatusChanged { ticket, status, .. } => {
            let record = ctx.lander.queue().record(ticket).await.ok()??;
            E::LandUpdate(common::ticket_proto(&rill_types::LandRequestRecord {
                status,
                ..record
            }))
        }
        _ => return None,
    };
    Some(wrap(change_id, event.at, mapped))
}
