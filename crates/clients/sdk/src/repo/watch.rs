//! Server-streamed watches over trunk, refs, the version log and the
//! event log.

use bytes::Bytes;
use futures::StreamExt;
use rill_proto::convert::id_to_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{TrunkSeq, Versionstamp};

use super::RepoClient;
use crate::error::{Error, Result};
use crate::stream::EventStream;
use crate::types::{EventQuery, LandEvent, LogEntry, LoggedEvent, RefEvent};

impl RepoClient {
    /// Landings as they happen, optionally resuming after a sequence.
    pub async fn watch_trunk(&self, after_seq: Option<TrunkSeq>) -> Result<EventStream<LandEvent>> {
        let stream = self
            .svc()
            .watch_trunk(pb::WatchTrunkRequest {
                repo: self.repo.clone(),
                after_seq: after_seq.map(TrunkSeq::get),
            })
            .await?;
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
            .await?;
        Ok(convert_stream(stream))
    }

    /// Every version stored from `after` on.
    pub async fn watch_log(&self, after: WatchStart) -> Result<EventStream<LogEntry>> {
        let stream = self
            .svc()
            .watch_log(pb::WatchLogRequest {
                repo: self.repo.clone(),
                after: after.to_proto(),
            })
            .await?;
        Ok(convert_stream(stream))
    }

    /// Every committed state change matching `query`, from `after` on.
    pub async fn watch_events(
        &self,
        after: WatchStart,
        query: EventQuery,
    ) -> Result<EventStream<LoggedEvent>> {
        let stream = self
            .svc()
            .watch_events(pb::WatchEventsRequest {
                repo: self.repo.clone(),
                after: after.to_proto(),
                kinds: query.kinds,
                change_id: query.change.map(|c| id_to_proto(&c)),
                ref_prefix: query.ref_prefix,
            })
            .await?;
        Ok(convert_stream(stream))
    }
}

/// A server stream of wire messages, as SDK values. A failure ends it.
pub(crate) fn convert_stream<V, W, T, E>(
    stream: connectrpc::client::ServerStream<crate::client::ResponseBody, V>,
) -> EventStream<T>
where
    V: buffa::MessageView<'static, Owned = W> + Send + 'static,
    W: buffa::view::HasMessageView<View<'static> = V>
        + buffa::Message
        + connectrpc::JsonDeserialize
        + Send
        + 'static,
    T: TryFrom<W, Error = E> + Send + 'static,
    E: Into<Error>,
{
    map_stream(stream, |msg| T::try_from(msg).map_err(Into::into))
}

/// A server stream of wire messages, each turned into an SDK value by
/// `f`. A failure ends it.
pub(crate) fn map_stream<V, W, T>(
    stream: connectrpc::client::ServerStream<crate::client::ResponseBody, V>,
    f: impl Fn(W) -> Result<T> + Send + Sync + 'static,
) -> EventStream<T>
where
    V: buffa::MessageView<'static, Owned = W> + Send + 'static,
    W: buffa::view::HasMessageView<View<'static> = V>
        + buffa::Message
        + connectrpc::JsonDeserialize
        + Send
        + 'static,
    T: Send + 'static,
{
    let f = std::sync::Arc::new(f);
    futures::stream::unfold(Some(stream), move |state| {
        let f = f.clone();
        async move {
            let mut stream = state?;
            match stream.message::<W>().await {
                Ok(Some(msg)) => Some((f(msg.to_owned_message()), Some(stream))),
                Ok(None) => None,
                Err(e) => Some((Err(e.into()), None)),
            }
        }
    })
    .boxed()
}

/// Where a log watch starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchStart {
    /// Only versions stored from now on.
    Now,
    /// The whole log.
    Beginning,
    /// After the entry with this cursor.
    After(Versionstamp),
}

impl WatchStart {
    fn to_proto(self) -> Option<Bytes> {
        match self {
            WatchStart::Now => None,
            WatchStart::Beginning => Some(Bytes::new()),
            WatchStart::After(cursor) => Some(Bytes::copy_from_slice(cursor.as_bytes())),
        }
    }
}
