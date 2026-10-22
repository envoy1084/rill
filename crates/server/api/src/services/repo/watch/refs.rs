//! `WatchRefs`: ref moves, including trunk moving on each landing.

use std::sync::Arc;

use rill_events::{EventFilter, Start};
use rill_proto::convert::{hash_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{EventKind, RefName, TrunkSeq};

use super::{forward, EventStream};
use crate::services::common::open;
use crate::services::repo::RepoSvc;
use rill_runtime::error::Result;
use rill_runtime::state::RepoContext;

impl RepoSvc {
    pub(in crate::services::repo) async fn watch_refs_impl(
        &self,
        request: tonic::Request<pb::WatchRefsRequest>,
    ) -> Result<EventStream<pb::RefEvent>> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read_any().await?;
        let prefix = request.get_ref().prefix.clone().unwrap_or_default();
        let trunk = RefName::trunk().to_string();
        let filter = EventFilter::kinds(["ref_moved", "landed"]);
        let ctx = opened.repo.clone();
        let log = rill_events::tail(ctx.repo.clone(), Start::Now, filter).await?;
        let reader = ctx.clone();
        Ok(forward(ctx, log, move |_, event| {
            let (ctx, prefix, trunk) = (reader.clone(), prefix.clone(), trunk.clone());
            async move {
                let (name, old, new) = match event.kind {
                    EventKind::RefMoved { name, old, new } => (name, old, new),
                    EventKind::Landed { seq, version, .. } => {
                        (trunk, trunk_before(&ctx, seq).await?, Some(version))
                    }
                    _ => return Ok(None),
                };
                if !name.starts_with(&prefix) {
                    return Ok(None);
                }
                Ok(Some(pb::RefEvent {
                    name,
                    old: old.map(|h| hash_to_proto(&h)),
                    new: new.map(|h| hash_to_proto(&h)),
                    at: timestamp_to_proto(event.at),
                }))
            }
        }))
    }
}

async fn trunk_before(
    ctx: &Arc<RepoContext>,
    seq: TrunkSeq,
) -> Result<Option<rill_types::ChangeVersionHash>> {
    match seq.get().checked_sub(1) {
        None | Some(0) => Ok(None),
        Some(prev) => Ok(ctx.repo.trunk_at(TrunkSeq(prev)).await?),
    }
}
