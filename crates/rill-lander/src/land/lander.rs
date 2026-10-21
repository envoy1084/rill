//! The lander: serves the stored land queue while it holds the lease.

use std::sync::Arc;

use rill_core::{CommitGraph, DeriveManager};
use rill_store::repo::{Lease, LeaseHolder};
use rill_store::RepoStore;
use rill_types::ChangeId;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::record::{list_landings, Landing};
use crate::config::LandPolicy;
use crate::error::Result;
use crate::events::LandEvent;
use crate::queue::{LandQueue, LandRequest, LandStatus, LandTicket, QueuedRequest, SWEEP_LIMIT};
use crate::traits::{ReviewHooks, Verifier};

/// Capacity of the event channel; slow subscribers lag rather than block
/// landing.
const EVENT_CAPACITY: usize = 1024;
/// How often `land_now` looks again while another lander serves the queue.
const STANDBY_POLL: std::time::Duration = std::time::Duration::from_millis(100);
/// How often finished requests past their retention are deleted.
const RETENTION_SWEEP: std::time::Duration = std::time::Duration::from_secs(3600);

/// Lands changes on trunk.
pub struct Lander {
    pub(super) repo: RepoStore,
    pub(super) derive: Arc<DeriveManager>,
    pub(super) graph: Arc<CommitGraph>,
    pub(super) hooks: Arc<dyn ReviewHooks>,
    pub(super) verifier: Arc<dyn Verifier>,
    pub(super) policy: LandPolicy,
    queue: LandQueue,
    events: broadcast::Sender<LandEvent>,
    /// The lander lease.
    lease: LeaseHolder,
}

/// The lease role landers fence their trunk writes with.
pub const LANDER_LEASE: &str = "lander";

impl Lander {
    /// A lander over `repo`.
    pub fn new(
        repo: RepoStore,
        derive: Arc<DeriveManager>,
        graph: Arc<CommitGraph>,
        hooks: Arc<dyn ReviewHooks>,
        verifier: Arc<dyn Verifier>,
        policy: LandPolicy,
    ) -> Arc<Self> {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Arc::new(Lander {
            lease: LeaseHolder::new(repo.clone(), LANDER_LEASE),
            queue: LandQueue::new(repo.clone()),
            repo,
            derive,
            graph,
            hooks,
            verifier,
            policy,
            events,
        })
    }

    /// The lander lease, taken or renewed as needed; `None` while another
    /// lander holds it. Holding it fences every other lander off trunk.
    pub async fn lease(&self) -> Result<Option<Lease>> {
        Ok(self.lease.current().await?)
    }

    /// The lease this lander believes it has, without renewing it.
    pub async fn held_lease(&self) -> Option<Lease> {
        self.lease.held().await
    }

    /// Drop the lease after it was found lost.
    pub(super) async fn lost_lease(&self) {
        self.lease.lost().await;
    }

    /// Give the lease up so a standby can take over at once.
    pub async fn release(&self) {
        self.lease.release().await;
    }

    /// The repository this lander lands on.
    pub fn repo(&self) -> &RepoStore {
        &self.repo
    }

    /// The derived-data manager.
    pub fn derive(&self) -> &Arc<DeriveManager> {
        &self.derive
    }

    /// The commit graph.
    pub fn graph(&self) -> &Arc<CommitGraph> {
        &self.graph
    }

    /// The review hooks.
    pub fn hooks(&self) -> &Arc<dyn ReviewHooks> {
        &self.hooks
    }

    /// The verifier.
    pub fn verifier(&self) -> &Arc<dyn Verifier> {
        &self.verifier
    }

    /// The policy in force.
    pub fn policy(&self) -> &LandPolicy {
        &self.policy
    }

    /// The request queue.
    pub fn queue(&self) -> &LandQueue {
        &self.queue
    }

    /// Subscribe to landing events.
    pub fn subscribe(&self) -> broadcast::Receiver<LandEvent> {
        self.events.subscribe()
    }

    /// Publish an event; used by the post-land worker too.
    pub fn publish(&self, event: LandEvent) {
        // No subscribers is not an error.
        let _ = self.events.send(event);
    }

    /// Queue a request and return its ticket; a change with an
    /// unfinished request gets that request's ticket.
    pub async fn enqueue(&self, request: LandRequest) -> Result<LandTicket> {
        let change_id = request.change_id;
        if let Some(ticket) = self.repo.active_land_request(&change_id).await? {
            return Ok(ticket);
        }
        if request.lane != rill_types::Lane::Emergency {
            let queued = self
                .queue
                .pending(SWEEP_LIMIT.max(self.policy.max_in_flight * 4))
                .await?
                .iter()
                .filter(|q| q.request.requester.id == request.requester.id)
                .count();
            if queued >= self.policy.max_in_flight {
                return Err(crate::error::Error::TooManyRequests {
                    requester: request.requester.name.clone(),
                    limit: self.policy.max_in_flight,
                });
            }
        }
        let (ticket, new) = self.queue.enqueue(&request).await?;
        if new {
            self.publish(LandEvent::Queued { ticket, change_id });
        }
        Ok(ticket)
    }

    /// Queue `request`, serve the queue, and return the request's status
    /// once it finishes, or after `max_wait` while another process holds
    /// the lease. For CLI, revert and test callers that want an answer.
    pub async fn land_now(self: &Arc<Self>, request: LandRequest) -> Result<LandStatus> {
        let ticket = self.enqueue(request).await?;
        let deadline = tokio::time::Instant::now() + self.policy.max_wait;
        loop {
            self.run_once().await?;
            let status = self
                .queue
                .status(ticket)
                .await?
                .ok_or(crate::error::Error::UnknownTicket(ticket.0))?;
            if status.is_terminal() || tokio::time::Instant::now() >= deadline {
                return Ok(status);
            }
            tokio::time::sleep(STANDBY_POLL).await;
        }
    }

    /// Serve every unfinished request, in queue order, as one sequence.
    /// Returns the number of requests taken up.
    pub async fn run_once(self: &Arc<Self>) -> Result<usize> {
        // A standby lander leaves requests queued until it holds the lease.
        let Some(lease) = self.lease().await? else {
            return Ok(0);
        };
        let queued = self.queue.pending(SWEEP_LIMIT).await?;
        if queued.is_empty() {
            return Ok(0);
        }
        Ok(self.sequence(queued, &lease).await)
    }

    /// Serve the queue until `shutdown` is cancelled: after each sweep,
    /// wait for a new request or `batch_window`, whichever comes first.
    pub async fn run_forever(self: Arc<Self>, shutdown: CancellationToken) {
        let mut last_retention = tokio::time::Instant::now();
        loop {
            let seen = self.repo.land_wake_count().await.unwrap_or(0);
            if let Err(e) = self.run_once().await {
                tracing::error!(error = %e, "landing sweep failed");
            }
            if last_retention.elapsed() >= RETENTION_SWEEP {
                last_retention = tokio::time::Instant::now();
                self.sweep_finished().await;
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = self.repo.wait_for_land_requests(seen, self.policy.batch_window) => {}
            }
        }
        self.release().await;
        tracing::info!("lander stopped");
    }

    async fn sweep_finished(&self) {
        let before = rill_types::Timestamp::from_millis(
            rill_types::Timestamp::now().millis()
                - crate::queue::TERMINAL_RETENTION.as_millis() as i64,
        );
        match self.repo.sweep_land_requests(before, 10_000).await {
            Ok(0) => {}
            Ok(n) => tracing::debug!(removed = n, "swept finished land requests"),
            Err(e) => tracing::warn!(error = %e, "sweeping land requests failed"),
        }
    }

    /// Recent landings, oldest first.
    pub async fn list_landings(&self) -> Result<Vec<Landing>> {
        list_landings(&self.repo).await
    }

    /// Mark a request failed, notify review and publish the event.
    pub(super) async fn fail(&self, queued: &QueuedRequest, reason: String) {
        let change_id = queued.request.change_id;
        tracing::warn!(%change_id, ticket = %queued.ticket, reason, "landing failed");
        self.set_status(
            queued,
            LandStatus::Failed {
                reason: reason.clone(),
            },
        )
        .await;
        self.return_to_owner(change_id, reason.clone()).await;
        self.publish(LandEvent::Failed { change_id, reason });
    }

    pub(super) async fn return_to_owner(&self, change_id: ChangeId, reason: String) {
        if let Err(e) = self.hooks.return_to_owner(change_id, reason).await {
            tracing::error!(%change_id, error = %e, "return_to_owner hook failed");
        }
    }

    /// Record `status` for `queued`, fenced by the lease this lander holds.
    /// A lander that lost the lease records nothing; its successor serves
    /// the request.
    pub(super) async fn set_status(&self, queued: &QueuedRequest, status: LandStatus) {
        let Some(lease) = self.held_lease().await else {
            return;
        };
        if let Err(e) = self
            .repo
            .set_land_status(
                queued.ticket,
                status,
                Some(&lease),
                &crate::revert::lander_principal(),
            )
            .await
        {
            tracing::warn!(ticket = %queued.ticket, error = %e, "recording a land status failed");
        }
    }
}
