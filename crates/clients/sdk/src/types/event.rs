//! Entries of a repository's event log.

use rill_proto::rill::v1 as pb;
use rill_types::{ChangeId, Event, Versionstamp};

use crate::error::Result;

/// One event, with the cursor to resume after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggedEvent {
    /// Pass to `watch_events` to resume after this event.
    pub cursor: Versionstamp,
    /// What happened.
    pub event: Event,
}

impl TryFrom<pb::WatchEventsResponse> for LoggedEvent {
    type Error = crate::error::Error;

    fn try_from(e: pb::WatchEventsResponse) -> Result<Self> {
        Ok(LoggedEvent {
            cursor: Versionstamp::from_slice(&e.cursor)?,
            event: rill_proto::convert::required("event_envelope.event", e.event)?.try_into()?,
        })
    }
}

/// Which events a watch wants; the default wants all of them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventQuery {
    /// Event kind names (`ref_moved`, `landed`, ...); empty means any.
    pub kinds: Vec<String>,
    /// Only events about this change.
    pub change: Option<ChangeId>,
    /// Only ref moves of refs starting with this prefix.
    pub ref_prefix: Option<String>,
}
