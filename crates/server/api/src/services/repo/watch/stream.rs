//! Forwarding the event log into gRPC server streams.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use futures::{Stream, StreamExt};
use rill_events::Start;
use rill_types::{Event, Versionstamp};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Status;

use rill_runtime::error::{Error, Result};
use rill_runtime::state::RepoContext;

/// A boxed server stream.
pub type EventStream<T> = Pin<Box<dyn Stream<Item = std::result::Result<T, Status>> + Send>>;

/// Buffer between the producer task and the gRPC stream.
pub(super) const STREAM_BUFFER: usize = 256;
/// How long a store-state watch sleeps between looks when nothing happens.
const WAIT: Duration = Duration::from_secs(5);

/// Where a watch with an `after` cursor starts: absent starts after the
/// newest event, empty replays from the start.
pub(crate) fn start_from(after: Option<&Bytes>) -> Result<Start> {
    Ok(match after {
        None => Start::Now,
        Some(bytes) if bytes.is_empty() => Start::Beginning,
        Some(bytes) => Start::After(
            Versionstamp::from_slice(bytes)
                .map_err(|e| Error::InvalidArgument(format!("after: {e}")))?,
        ),
    })
}

/// Stream what `map` makes of each event of `log` until the repository
/// shuts down or the client goes away; `map` returning `None` skips one.
pub(crate) fn forward<L, T, F, Fut>(ctx: Arc<RepoContext>, log: L, mut map: F) -> EventStream<T>
where
    L: Stream<Item = rill_events::Result<(Versionstamp, Event)>> + Send + 'static,
    T: Send + 'static,
    F: FnMut(Versionstamp, Event) -> Fut + Send + 'static,
    Fut: Future<Output = Result<Option<T>>> + Send,
{
    let (tx, out) = mpsc::channel(STREAM_BUFFER);
    tokio::spawn(async move {
        let mut log = Box::pin(log);
        loop {
            let next = tokio::select! {
                _ = ctx.shutdown.cancelled() => break,
                next = log.next() => next,
            };
            let item = match next {
                None => break,
                Some(Err(e)) => Err(Error::from(e)),
                Some(Ok((cursor, event))) => map(cursor, event).await,
            };
            let item = match item {
                Ok(None) => continue,
                Ok(Some(item)) => Ok(item),
                Err(e) => Err(Status::from(e)),
            };
            let failed = item.is_err();
            if tx.send(item).await.is_err() || failed {
                break;
            }
        }
    });
    Box::pin(ReceiverStream::new(out))
}

/// Wait until an event after count `seen` commits; `false` once the
/// repository shuts down.
pub(super) async fn wait_for_events(ctx: &RepoContext, seen: i64) -> Result<bool> {
    tokio::select! {
        _ = ctx.shutdown.cancelled() => Ok(false),
        r = ctx.repo.wait_for_events(seen, WAIT) => r.map(|_| true).map_err(Error::from),
    }
}
