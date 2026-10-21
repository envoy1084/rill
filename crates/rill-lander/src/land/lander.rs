//! The lander: owns the queue and drives requests through the pipeline.

use std::sync::Arc;

use rill_core::{CommitGraph, DeriveManager};
use rill_store::repo::{Lease, LeaseHolder};
use rill_store::RepoStore;
use rill_types::ChangeId;
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::record::{list_landings, Landing};
use crate::config::LandPolicy;
use crate::conflict_graph::touched_paths;
use crate::error::Result;
use crate::events::LandEvent;
use crate::queue::{LandQueue, LandRequest, LandStatus, LandTicket, QueuedRequest};
use crate::traits::{ReviewHooks, Verifier};

/// Capacity of the event channel; slow subscribers lag rather than block
/// landing.
const EVENT_CAPACITY: usize = 1024;

/// Lands changes on trunk.
pub struct Lander {
    pub(super) repo: RepoStore,
    pub(super) derive: Arc<DeriveManager>,
    pub(super) graph: Arc<CommitGraph>,
    pub(super) hooks: Arc<dyn ReviewHooks>,
    pub(super) verifier: Arc<dyn Verifier>,
    pub(super) policy: LandPolicy,
    queue: Arc<LandQueue>,
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
            repo,
            derive,
            graph,
            hooks,
            verifier,
            policy,
            queue: Arc::new(LandQueue::new()),
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
    pub fn queue(&self) -> &Arc<LandQueue> {
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

    /// Queue a request and return its ticket.
    pub async fn enqueue(&self, request: LandRequest) -> LandTicket {
        let change_id = request.change_id;
        let ticket = self.queue.enqueue(request).await;
        self.publish(LandEvent::Queued { ticket, change_id });
        ticket
    }

    /// Queue `request`, process the queue, and return the request's final
    /// status. For CLI and test callers that want a synchronous answer.
    pub async fn land_now(self: &Arc<Self>, request: LandRequest) -> Result<LandStatus> {
        let ticket = self.enqueue(request).await;
        let mut rx = self
            .queue
            .watch(ticket)
            .ok_or(crate::error::Error::UnknownTicket(ticket.0))?;
        self.run_once().await?;
        loop {
            let status = rx.borrow_and_update().clone();
            if status.is_terminal() {
                return Ok(status);
            }
            if rx.changed().await.is_err() {
                return Ok(status);
            }
        }
    }

    /// Process everything currently queued. Independent requests are
    /// handled concurrently; overlapping ones in arrival order. Returns
    /// the number of requests processed.
    pub async fn run_once(self: &Arc<Self>) -> Result<usize> {
        // A standby lander leaves requests queued until it holds the lease.
        if self.lease().await?.is_none() {
            return Ok(0);
        }
        let queued = self.queue.drain().await;
        if queued.is_empty() {
            return Ok(0);
        }
        let count = queued.len();
        let batches = self.partition(queued).await;
        let tasks: Vec<_> = batches
            .into_iter()
            .map(|batch| {
                let lander = self.clone();
                tokio::spawn(async move {
                    for queued in batch {
                        lander.process(&queued).await;
                    }
                })
            })
            .collect();
        for task in tasks {
            task.await?;
        }
        Ok(count)
    }

    /// Group requests by touched paths; a request whose paths cannot be
    /// computed (its version is missing) fails on the spot.
    async fn partition(&self, queued: Vec<QueuedRequest>) -> Vec<Vec<QueuedRequest>> {
        let mut items = Vec::with_capacity(queued.len());
        for request in queued {
            match touched_paths(&self.derive, &request.request.version).await {
                Ok(paths) => items.push((request, paths)),
                Err(e) => {
                    self.fail(&request, format!("cannot compute touched paths: {e}"))
                        .await
                }
            }
        }
        crate::conflict_graph::partition(items)
    }

    /// Sweep the queue every `batch_window` until `shutdown` is cancelled.
    pub async fn run_forever(self: Arc<Self>, shutdown: CancellationToken) {
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(self.policy.batch_window) => {
                    if let Err(e) = self.run_once().await {
                        tracing::error!(error = %e, "landing sweep failed");
                    }
                    self.queue.forget_terminal(crate::queue::TERMINAL_RETENTION);
                }
            }
        }
        self.release().await;
        tracing::info!("lander stopped");
    }

    /// Recent landings, oldest first.
    pub async fn list_landings(&self) -> Result<Vec<Landing>> {
        list_landings(&self.repo).await
    }

    /// Mark a request failed, notify review and publish the event.
    pub(super) async fn fail(&self, queued: &QueuedRequest, reason: String) {
        let change_id = queued.request.change_id;
        tracing::warn!(%change_id, ticket = %queued.ticket, reason, "landing failed");
        self.queue.set_status(
            queued.ticket,
            LandStatus::Failed {
                reason: reason.clone(),
            },
        );
        self.return_to_owner(change_id, reason.clone()).await;
        self.publish(LandEvent::Failed { change_id, reason });
    }

    pub(super) async fn return_to_owner(&self, change_id: ChangeId, reason: String) {
        if let Err(e) = self.hooks.return_to_owner(change_id, reason).await {
            tracing::error!(%change_id, error = %e, "return_to_owner hook failed");
        }
    }

    pub(super) fn set_status(&self, ticket: LandTicket, status: LandStatus) {
        self.queue.set_status(ticket, status);
    }
}
