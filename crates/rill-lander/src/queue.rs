//! The in-memory landing queue.
//!
//! Requests are accepted in arrival order and handed to the lander in
//! batches. Status is kept per ticket for the life of the process; the
//! durable record of what landed is the trunk log and the landing
//! records, not this queue.

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use rill_types::{
    ChangeId, ChangeVersionHash, Principal, RepoPathBuf, RiskClass, Timestamp, TrunkSeq,
};
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, Mutex};

use crate::config::VerifyMode;

/// A request to land one version of a change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LandRequest {
    pub change_id: ChangeId,
    pub version: ChangeVersionHash,
    pub requester: Principal,
    pub risk: RiskClass,
    pub requested_at: Timestamp,
    /// Overrides the policy's verification mode for this request; used by
    /// reverts, which land without pre-land verification.
    pub verify_override: Option<VerifyMode>,
}

impl LandRequest {
    /// A request for `version` of `change_id` by `requester` at `risk`,
    /// timestamped now.
    pub fn new(
        change_id: ChangeId,
        version: ChangeVersionHash,
        requester: Principal,
        risk: RiskClass,
    ) -> Self {
        LandRequest {
            change_id,
            version,
            requester,
            risk,
            requested_at: Timestamp::now(),
            verify_override: None,
        }
    }
}

/// Handle to a queued request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct LandTicket(pub u64);

impl fmt::Display for LandTicket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "land#{}", self.0)
    }
}

/// Where a request is in the pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LandStatus {
    Queued,
    Verifying,
    Landing,
    Landed { seq: TrunkSeq },
    Failed { reason: String },
    Conflict { paths: Vec<RepoPathBuf> },
    Returned { reason: String },
}

impl LandStatus {
    /// True once the request will not change state again.
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            LandStatus::Landed { .. }
                | LandStatus::Failed { .. }
                | LandStatus::Conflict { .. }
                | LandStatus::Returned { .. }
        )
    }
}

/// A request together with its ticket.
#[derive(Debug, Clone)]
pub struct QueuedRequest {
    pub ticket: LandTicket,
    pub request: LandRequest,
}

/// How long a finished ticket's outcome stays queryable, so a client that
/// polls after the request completed still sees why it failed.
pub const TERMINAL_RETENTION: Duration = Duration::from_secs(600);

/// FIFO of pending requests plus per-ticket status.
pub struct LandQueue {
    pending: Mutex<VecDeque<QueuedRequest>>,
    status: DashMap<LandTicket, watch::Sender<LandStatus>>,
    finished: DashMap<LandTicket, Instant>,
    next_ticket: AtomicU64,
}

impl Default for LandQueue {
    fn default() -> Self {
        LandQueue::new()
    }
}

impl LandQueue {
    /// An empty queue.
    pub fn new() -> Self {
        LandQueue {
            pending: Mutex::new(VecDeque::new()),
            status: DashMap::new(),
            finished: DashMap::new(),
            next_ticket: AtomicU64::new(1),
        }
    }

    /// Accept a request and return its ticket.
    pub async fn enqueue(&self, request: LandRequest) -> LandTicket {
        let ticket = LandTicket(self.next_ticket.fetch_add(1, Ordering::SeqCst));
        let (tx, _rx) = watch::channel(LandStatus::Queued);
        self.status.insert(ticket, tx);
        self.pending
            .lock()
            .await
            .push_back(QueuedRequest { ticket, request });
        ticket
    }

    /// Current status of a ticket, if it is known.
    pub fn status(&self, ticket: LandTicket) -> Option<LandStatus> {
        self.status.get(&ticket).map(|tx| tx.borrow().clone())
    }

    /// A receiver that observes every status change of a ticket.
    pub fn watch(&self, ticket: LandTicket) -> Option<watch::Receiver<LandStatus>> {
        self.status.get(&ticket).map(|tx| tx.subscribe())
    }

    /// Record a new status. Unknown tickets are ignored.
    pub fn set_status(&self, ticket: LandTicket, status: LandStatus) {
        if let Some(tx) = self.status.get(&ticket) {
            if status.is_terminal() {
                self.finished.entry(ticket).or_insert_with(Instant::now);
            }
            // `send` fails when no receiver exists; the status must be
            // recorded whether or not anyone is watching.
            tx.send_replace(status);
        }
    }

    /// Remove and return every pending request, oldest first.
    pub async fn drain(&self) -> Vec<QueuedRequest> {
        self.pending.lock().await.drain(..).collect()
    }

    /// Number of requests waiting to be processed.
    pub async fn len(&self) -> usize {
        self.pending.lock().await.len()
    }

    /// True when nothing is waiting.
    pub async fn is_empty(&self) -> bool {
        self.len().await == 0
    }

    /// Forget tickets that finished more than `retention` ago, keeping
    /// memory bounded in long-running servers.
    pub fn forget_terminal(&self, retention: Duration) {
        self.finished.retain(|ticket, at| {
            let keep = at.elapsed() < retention;
            if !keep {
                self.status.remove(ticket);
            }
            keep
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enqueue_status_watch() {
        let queue = LandQueue::new();
        let request = LandRequest::new(
            ChangeId::new(),
            ChangeVersionHash::ZERO,
            Principal::human("a"),
            RiskClass::Normal,
        );
        let ticket = queue.enqueue(request).await;
        assert_eq!(queue.status(ticket), Some(LandStatus::Queued));
        queue.set_status(ticket, LandStatus::Verifying);
        assert_eq!(queue.status(ticket), Some(LandStatus::Verifying));
        let mut rx = queue.watch(ticket).unwrap();
        queue.set_status(ticket, LandStatus::Landed { seq: TrunkSeq(1) });
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), LandStatus::Landed { seq: TrunkSeq(1) });
        assert_eq!(queue.drain().await.len(), 1);
        assert!(queue.is_empty().await);
        queue.forget_terminal(TERMINAL_RETENTION);
        assert_eq!(
            queue.status(ticket),
            Some(LandStatus::Landed { seq: TrunkSeq(1) })
        );
        queue.forget_terminal(Duration::ZERO);
        assert!(queue.status(ticket).is_none());
    }
}
