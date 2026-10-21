//! The landing queue, kept in the store: requests survive restarts, any
//! server can accept one and report its status, and whichever lander holds
//! the lease serves them (see `RepoStore::queue_land`).

use std::time::Duration;

use rill_store::RepoStore;
pub use rill_types::{LandRequest, LandRequestRecord, LandStatus, LandTicket};

use crate::error::Result;

/// How long a finished request stays queryable.
pub const TERMINAL_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Unfinished requests read per sweep.
pub const SWEEP_LIMIT: usize = 256;

/// An unfinished request as the lander works on it.
#[derive(Debug, Clone)]
pub struct QueuedRequest {
    pub ticket: LandTicket,
    pub request: LandRequest,
    pub(crate) record: LandRequestRecord,
}

impl QueuedRequest {
    /// The stored record as it was read.
    pub fn record(&self) -> LandRequestRecord {
        self.record.clone()
    }
}

impl From<LandRequestRecord> for QueuedRequest {
    fn from(record: LandRequestRecord) -> Self {
        QueuedRequest {
            ticket: record.ticket,
            request: record.request.clone(),
            record,
        }
    }
}

/// A handle on one repository's land requests.
#[derive(Clone)]
pub struct LandQueue {
    repo: RepoStore,
}

impl LandQueue {
    pub fn new(repo: RepoStore) -> Self {
        LandQueue { repo }
    }

    /// Queue a request and return its ticket; a change with an unfinished
    /// request gets that request's ticket.
    pub async fn enqueue(&self, request: &LandRequest) -> Result<(LandTicket, bool)> {
        Ok(self.repo.queue_land(request).await?)
    }

    /// The request with `ticket`, if it exists.
    pub async fn record(&self, ticket: LandTicket) -> Result<Option<LandRequestRecord>> {
        Ok(self.repo.land_request(ticket).await?)
    }

    /// The current status of `ticket`, if it exists.
    pub async fn status(&self, ticket: LandTicket) -> Result<Option<LandStatus>> {
        Ok(self.record(ticket).await?.map(|r| r.status))
    }

    /// Up to `limit` unfinished requests in the order they are served.
    pub async fn pending(&self, limit: usize) -> Result<Vec<QueuedRequest>> {
        Ok(self
            .repo
            .pending_land_requests(limit)
            .await?
            .into_iter()
            .map(QueuedRequest::from)
            .collect())
    }

    /// True when no request is unfinished.
    pub async fn is_empty(&self) -> Result<bool> {
        Ok(self.repo.pending_land_requests(1).await?.is_empty())
    }
}
