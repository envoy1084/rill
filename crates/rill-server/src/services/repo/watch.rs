//! Server-streaming watches over the repository event bus.
//!
//! Each watch subscribes to the bus *before* replaying from the store, so
//! nothing is missed between the replay and the live tail. Bus events are
//! treated as wake-ups: the stream then reads what it reports from the
//! store (log, trunk, refs), which keeps the output exact even when the
//! lossy bus lags.

use std::pin::Pin;
use std::sync::Arc;

use bytes::Bytes;
use futures::Stream;
use rill_proto::convert::{hash_to_proto, timestamp_to_proto};
use rill_proto::rill::v1 as pb;
use rill_types::{RepoPathBuf, TrunkSeq, Versionstamp};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use super::RepoSvc;
use crate::error::{Error, Result};
use crate::services::common::{self, open};
use crate::state::{RepoContext, RepoEvent};

/// A boxed server stream.
pub type EventStream<T> = Pin<Box<dyn Stream<Item = std::result::Result<T, Status>> + Send>>;

/// Buffer between the producer task and the gRPC stream.
const STREAM_BUFFER: usize = 256;
/// How many log entries a replay fetches per store read.
const REPLAY_PAGE: usize = 512;

impl RepoSvc {
    pub(super) async fn watch_refs_impl(
        &self,
        request: tonic::Request<pb::WatchRefsRequest>,
    ) -> Result<EventStream<pb::RefEvent>> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let prefix = request.get_ref().prefix.clone().unwrap_or_default();
        let ctx = opened.repo.clone();
        let mut rx = ctx.events.subscribe();
        let (tx, out) = mpsc::channel(STREAM_BUFFER);
        tokio::spawn(async move {
            loop {
                let received = tokio::select! {
                    _ = ctx.shutdown.cancelled() => break,
                    r = rx.recv() => r,
                };
                let event = match received {
                    Ok(RepoEvent::RefMoved { name, old, new, at })
                        if name.as_str().starts_with(&prefix) =>
                    {
                        pb::RefEvent {
                            name: name.to_string(),
                            old: old.map(|h| hash_to_proto(&h)),
                            new: new.map(|h| hash_to_proto(&h)),
                            at: timestamp_to_proto(at),
                        }
                    }
                    Ok(_) | Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                };
                if tx.send(Ok(event)).await.is_err() {
                    break;
                }
            }
        });
        Ok(Box::pin(ReceiverStream::new(out)))
    }

    pub(super) async fn watch_trunk_impl(
        &self,
        request: tonic::Request<pb::WatchTrunkRequest>,
    ) -> Result<EventStream<pb::LandEvent>> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let ctx = opened.repo.clone();
        let mut rx = ctx.events.subscribe();
        // Without `after_seq` the watch starts at the current head: only
        // new landings are delivered.
        let mut cursor = match request.get_ref().after_seq {
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
            loop {
                if let Err(e) = drain_trunk(&ctx, &mut cursor, &tx).await {
                    let _ = tx.send(Err(e.into())).await;
                    break;
                }
                let received = tokio::select! {
                    _ = ctx.shutdown.cancelled() => break,
                    r = rx.recv() => r,
                };
                match received {
                    Ok(_) | Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        });
        Ok(Box::pin(ReceiverStream::new(out)))
    }

    pub(super) async fn watch_log_impl(
        &self,
        request: tonic::Request<pb::WatchLogRequest>,
    ) -> Result<EventStream<pb::LogEntry>> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let ctx = opened.repo.clone();
        let mut rx = ctx.events.subscribe();
        let mut cursor = match &request.get_ref().after {
            None => ctx.repo.log_head().await?,
            Some(bytes) if bytes.is_empty() => None,
            Some(bytes) => Some(
                Versionstamp::from_slice(bytes)
                    .map_err(|e| Error::InvalidArgument(format!("after: {e}")))?,
            ),
        };
        let (tx, out) = mpsc::channel(STREAM_BUFFER);
        tokio::spawn(async move {
            loop {
                if let Err(e) = drain_log(&ctx, &mut cursor, &tx).await {
                    let _ = tx.send(Err(e.into())).await;
                    break;
                }
                let received = tokio::select! {
                    _ = ctx.shutdown.cancelled() => break,
                    r = rx.recv() => r,
                };
                match received {
                    Ok(_) | Err(RecvError::Lagged(_)) => continue,
                    Err(RecvError::Closed) => break,
                }
            }
        });
        Ok(Box::pin(ReceiverStream::new(out)))
    }
}

async fn drain_trunk(
    ctx: &Arc<RepoContext>,
    cursor: &mut TrunkSeq,
    tx: &mpsc::Sender<std::result::Result<pb::LandEvent, Status>>,
) -> Result<()> {
    let Some((head, _)) = ctx.repo.trunk_head().await? else {
        return Ok(());
    };
    if head <= *cursor {
        return Ok(());
    }
    for (seq, hash) in ctx.repo.trunk_range(*cursor, head).await? {
        let event = common::land_event(ctx, seq, hash).await?;
        if tx.send(Ok(event)).await.is_err() {
            return Ok(());
        }
        *cursor = seq;
    }
    Ok(())
}

async fn drain_log(
    ctx: &Arc<RepoContext>,
    cursor: &mut Option<Versionstamp>,
    tx: &mpsc::Sender<std::result::Result<pb::LogEntry, Status>>,
) -> Result<()> {
    loop {
        let page = ctx.repo.log_after(*cursor, REPLAY_PAGE).await?;
        if page.is_empty() {
            return Ok(());
        }
        for (stamp, hash) in page {
            let entry = pb::LogEntry {
                cursor: Bytes::copy_from_slice(stamp.as_bytes()),
                info: Some(common::change_info(ctx, &hash).await?),
            };
            if tx.send(Ok(entry)).await.is_err() {
                return Ok(());
            }
            *cursor = Some(stamp);
        }
    }
}
