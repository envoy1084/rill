//! `WatchTrunk` and `WatchVersions`: landings and stored versions, read from
//! the store each time the event counter moves.

use std::sync::Arc;

use bytes::Bytes;
use rill_proto::rill::v1 as pb;
use rill_types::{TrunkSeq, Versionstamp};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use super::stream::{start_from, wait_for_events, EventStream, STREAM_BUFFER};
use crate::services::common::{self, open_repo};
use crate::services::repo::RepoSvc;
use crate::services::Call;
use connectrpc::ConnectError;
use rill_runtime::error::Result;
use rill_runtime::state::RepoContext;

/// How many log entries a replay fetches per store read.
const REPLAY_PAGE: usize = 512;

type Sender<T> = mpsc::Sender<std::result::Result<T, ConnectError>>;

impl RepoSvc {
    pub(in crate::services::repo) async fn watch_trunk_impl(
        &self,
        request: Call<pb::WatchTrunkRequest>,
    ) -> Result<EventStream<pb::WatchTrunkResponse>> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().read_any().await?;
        let ctx = opened.repo.clone();
        // Without `after_position` the watch starts at the current head:
        // only new landings are delivered.
        let mut cursor = match request.get_ref().after_position {
            Some(seq) => TrunkSeq(seq),
            None => ctx
                .repo
                .trunk_head()
                .await?
                .map(|(s, _)| s)
                .unwrap_or(TrunkSeq::ZERO),
        };
        let (tx, out) = mpsc::channel(STREAM_BUFFER);
        tokio::spawn(async move {
            let result: Result<()> = async {
                loop {
                    let seen = ctx.repo.event_count().await?;
                    if !drain_trunk(&ctx, &mut cursor, &tx).await? {
                        return Ok(());
                    }
                    if !wait_for_events(&ctx, seen).await? {
                        return Ok(());
                    }
                }
            }
            .await;
            if let Err(e) = result {
                let _ = tx.send(Err(ConnectError::from(e))).await;
            }
        });
        Ok(Box::pin(ReceiverStream::new(out)))
    }

    pub(in crate::services::repo) async fn watch_versions_impl(
        &self,
        request: Call<pb::WatchVersionsRequest>,
    ) -> Result<EventStream<pb::WatchVersionsResponse>> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        let filter = opened.permit().read_any().await?;
        let ctx = opened.repo.clone();
        let mut cursor = match start_from(request.get_ref().after.as_ref())? {
            rill_events::Start::Now => ctx.repo.log_head().await?,
            rill_events::Start::Beginning => None,
            rill_events::Start::After(stamp) => Some(stamp),
        };
        let (tx, out) = mpsc::channel(STREAM_BUFFER);
        tokio::spawn(async move {
            let result: Result<()> = async {
                loop {
                    let seen = ctx.repo.event_count().await?;
                    if !drain_log(&ctx, &filter, &mut cursor, &tx).await? {
                        return Ok(());
                    }
                    if !wait_for_events(&ctx, seen).await? {
                        return Ok(());
                    }
                }
            }
            .await;
            if let Err(e) = result {
                let _ = tx.send(Err(ConnectError::from(e))).await;
            }
        });
        Ok(Box::pin(ReceiverStream::new(out)))
    }
}

/// Send the landings after `cursor`; `false` once the client is gone.
async fn drain_trunk(
    ctx: &Arc<RepoContext>,
    cursor: &mut TrunkSeq,
    tx: &Sender<pb::WatchTrunkResponse>,
) -> Result<bool> {
    let Some((head, _)) = ctx.repo.trunk_head().await? else {
        return Ok(true);
    };
    if head <= *cursor {
        return Ok(true);
    }
    for (seq, hash) in ctx.repo.trunk_range(*cursor, head).await? {
        let landing = common::landing(ctx, seq, hash).await?;
        let response = pb::WatchTrunkResponse {
            landing: landing.into(),
        };
        if tx.send(Ok(response)).await.is_err() {
            return Ok(false);
        }
        *cursor = seq;
    }
    Ok(true)
}

/// Send the log entries after `cursor`; `false` once the client is gone.
async fn drain_log(
    ctx: &Arc<RepoContext>,
    filter: &rill_policy::PathFilter,
    cursor: &mut Option<Versionstamp>,
    tx: &Sender<pb::WatchVersionsResponse>,
) -> Result<bool> {
    loop {
        let page = ctx.repo.log_after(*cursor, REPLAY_PAGE).await?;
        if page.is_empty() {
            return Ok(true);
        }
        for (stamp, hash) in page {
            *cursor = Some(stamp);
            let Some(info) = crate::services::changes::visible::info_of(ctx, filter, &hash).await?
            else {
                continue;
            };
            let entry = pb::WatchVersionsResponse {
                cursor: Bytes::copy_from_slice(stamp.as_bytes()),
                version: common::change_info_proto(ctx, &info).into(),
            };
            if tx.send(Ok(entry)).await.is_err() {
                return Ok(false);
            }
        }
    }
}
