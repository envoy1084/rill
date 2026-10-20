//! Server-streamed watches over trunk, refs and the log.

use bytes::Bytes;
use futures::StreamExt;
use rill_proto::rill::v1 as pb;
use rill_types::{TrunkSeq, Versionstamp};
use tonic::Streaming;

use super::RepoClient;
use crate::error::{Error, Result};
use crate::stream::EventStream;
use crate::types::{LandEvent, LogEntry, RefEvent};

impl RepoClient {
    /// Landings as they happen, optionally resuming after a sequence.
    pub async fn watch_trunk(&self, after_seq: Option<TrunkSeq>) -> Result<EventStream<LandEvent>> {
        let stream = self
            .svc()
            .watch_trunk(pb::WatchTrunkRequest {
                repo: self.repo.clone(),
                after_seq: after_seq.map(TrunkSeq::get),
            })
            .await?
            .into_inner();
        Ok(convert_stream(stream))
    }

    /// Ref moves, optionally restricted to a name prefix.
    pub async fn watch_refs(&self, prefix: Option<&str>) -> Result<EventStream<RefEvent>> {
        let stream = self
            .svc()
            .watch_refs(pb::WatchRefsRequest {
                repo: self.repo.clone(),
                prefix: prefix.map(str::to_string),
            })
            .await?
            .into_inner();
        Ok(convert_stream(stream))
    }

    /// Every version stored from `after` on.
    pub async fn watch_log(&self, after: WatchStart) -> Result<EventStream<LogEntry>> {
        let after = match after {
            WatchStart::Now => None,
            WatchStart::Beginning => Some(Bytes::new()),
            WatchStart::After(cursor) => Some(Bytes::copy_from_slice(cursor.as_bytes())),
        };
        let stream = self
            .svc()
            .watch_log(pb::WatchLogRequest {
                repo: self.repo.clone(),
                after,
            })
            .await?
            .into_inner();
        Ok(convert_stream(stream))
    }
}

/// Map a tonic stream of wire messages into SDK values.
pub(crate) fn convert_stream<W, T, E>(stream: Streaming<W>) -> EventStream<T>
where
    W: Send + 'static,
    T: TryFrom<W, Error = E> + Send + 'static,
    E: Into<Error>,
{
    stream
        .map(|item| match item {
            Ok(msg) => T::try_from(msg).map_err(Into::into),
            Err(status) => Err(status.into()),
        })
        .boxed()
}

/// Where a version-log watch starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchStart {
    /// Only versions stored from now on.
    Now,
    /// The whole log.
    Beginning,
    /// After the entry with this cursor.
    After(Versionstamp),
}
