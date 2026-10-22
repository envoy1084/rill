//! `WatchReview`: one change's review and land-request events, from the
//! event log.

use rill_events::{EventFilter, Start};
use rill_proto::convert::{hash_to_proto, id_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, Event, EventKind, LabelVote, RepoPathBuf, ReviewStatus, Timestamp};

use super::reviews::thread_proto;
use super::ReviewSvc;
use crate::services::common::{self, open_repo};
use crate::services::repo::forward;
use crate::services::{names, Call};
use rill_runtime::error::Result;
use rill_runtime::state::RepoContext;

/// Server stream of review events.
pub type ReviewEventStream = crate::services::repo::EventStream<pb::WatchReviewResponse>;

impl ReviewSvc {
    pub(super) async fn watch_review_impl(
        &self,
        request: Call<pb::WatchReviewRequest>,
    ) -> Result<ReviewEventStream> {
        let name = &request.get_ref().name;
        let opened = open_repo(&self.state, &request, name).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let id = names::review_in(&opened.repo, name)?;
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

fn wrap(
    change_id: &ChangeId,
    at: Timestamp,
    event: pb::watch_review_response::Event,
) -> pb::WatchReviewResponse {
    pb::WatchReviewResponse {
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
) -> Option<pb::WatchReviewResponse> {
    use pb::watch_review_response::Event as E;
    let status = |s: ReviewStatus| E::StatusChanged(pb::ReviewStatus::from(s).into());
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
            E::ThreadUpdated(thread_proto(ctx, change_id, thread))
        }
        EventKind::VersionStored { version, .. } => E::NewVersion(hash_to_proto(&version)),
        EventKind::LandRequested { ticket, .. } => {
            let record = ctx.lander.queue().record(ticket).await.ok()??;
            E::LandUpdate(common::land_request_proto(
                ctx,
                &rill_types::LandRequestRecord {
                    status: rill_types::LandStatus::Queued,
                    ..record
                },
            ))
        }
        EventKind::LandStatusChanged { ticket, status, .. } => {
            let record = ctx.lander.queue().record(ticket).await.ok()??;
            E::LandUpdate(common::land_request_proto(
                ctx,
                &rill_types::LandRequestRecord { status, ..record },
            ))
        }
        _ => return None,
    };
    Some(wrap(change_id, event.at, mapped))
}
