//! `WatchEvents`: the event log itself.

use bytes::Bytes;
use rill_events::EventFilter;
use rill_proto::convert::opt_id_from_proto;
use rill_proto::rill::v1 as pb;
use rill_types::RepoPathBuf;

use super::stream::{forward, start_from, EventStream};
use crate::services::common::open;
use crate::services::repo::RepoSvc;
use crate::services::Call;
use rill_runtime::error::Result;

impl RepoSvc {
    pub(in crate::services::repo) async fn watch_events_impl(
        &self,
        request: Call<pb::WatchEventsRequest>,
    ) -> Result<EventStream<pb::EventEnvelope>> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
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
            Ok(Some(pb::EventEnvelope {
                cursor: Bytes::copy_from_slice(cursor.as_bytes()),
                event: pb::Event::from(&event).into(),
            }))
        }))
    }
}
