//! `rill.v1.EventService`: the repository event log, filtered and
//! resumable by cursor.

use std::sync::Arc;

use bytes::Bytes;
use connectrpc::{RequestContext, Response, ServiceRequest};
use rill_events::EventFilter;
use rill_proto::convert::opt_id_from_proto;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::EventService;
use rill_types::RepoPathBuf;

use crate::services::common::{open_repo, GrpcResult};
use crate::services::repo::{forward, start_from, EventStream};
use crate::services::Call;
use rill_runtime::error::Result;
use rill_runtime::state::AppState;

/// The event service.
#[derive(Clone)]
pub struct EventSvc {
    state: Arc<AppState>,
}

impl EventSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        EventSvc { state }
    }

    async fn watch_events_impl(
        &self,
        request: Call<pb::WatchEventsRequest>,
    ) -> Result<EventStream<pb::WatchEventsResponse>> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let req = request.get_ref();
        let filter = EventFilter {
            kinds: req.kinds.clone(),
            change: opt_id_from_proto("change_id", req.change_id.as_ref())?,
            ref_prefix: req.ref_prefix.clone(),
        };
        let start = start_from(req.after.as_ref())?;
        let ctx = opened.repo.clone();
        let log = rill_events::tail(ctx.repo.clone(), start, filter).await?;
        Ok(forward(ctx, log, |cursor, event| async move {
            Ok(Some(pb::WatchEventsResponse {
                cursor: Bytes::copy_from_slice(cursor.as_bytes()),
                event: pb::Event::from(&event).into(),
            }))
        }))
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl EventService for EventSvc {
    async fn watch_events(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchEventsRequest>,
    ) -> GrpcResult<EventStream<pb::WatchEventsResponse>> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_events_impl(request).await?))
    }
}
