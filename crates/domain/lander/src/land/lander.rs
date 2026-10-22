//! The lander: serves the stored land queue while it holds the lease.

use std::sync::Arc;

use rill_core::{CommitGraph, DeriveManager};
use rill_store::repo::{Lease, LeaseHolder};
use rill_store::RepoStore;
use rill_types::{ChangeId, GroupMode, Line};
use tokio::sync::broadcast;
use tokio_util::sync::CancellationToken;

use super::record::{list_landings, Landing};
use super::sequence::MAX_BATCH;
use crate::config::LandPolicy;
use crate::error::Result;
use crate::events::LandEvent;
use crate::queue::{LandQueue, LandRequest, LandStatus, LandTicket, QueuedRequest, SWEEP_LIMIT};
use crate::schedule::{keep_groups_together, serve_order, share_key, Eta, Thrash, Timings};
use crate::traits::{
    ConflictResolver, FixDispatcher, NoRegenerator, Regenerator, ReviewHooks, TargetGraph, Verifier,
};

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
    pub(crate) hooks: Arc<dyn ReviewHooks>,
    pub(super) verifier: Arc<dyn Verifier>,
    pub(crate) regenerator: Arc<dyn Regenerator>,
    pub(super) resolver: Option<Arc<dyn ConflictResolver>>,
    /// Offered new trunk incidents, when the server has fix agents.
    pub(crate) fixer: Option<Arc<dyn FixDispatcher>>,
    /// Build targets as conflict domains, when the server has a graph.
    pub(super) targets: Option<Arc<dyn TargetGraph>>,
    /// Targets each version touches, by version.
    pub(super) touched: parking_lot::Mutex<super::domains::TouchedCache>,
    pub(super) policy: LandPolicy,
    /// What it lands on.
    pub(super) line: Line,
    queue: LandQueue,
    events: broadcast::Sender<LandEvent>,
    /// The lander lease.
    lease: LeaseHolder,
    /// Rules by the trunk head they were read at.
    pub(super) rules:
        parking_lot::Mutex<Option<(Option<rill_types::ChangeVersionHash>, Arc<super::HeadRules>)>>,
    /// Recent rebase outcomes per domain.
    pub(super) thrash: Thrash,
    /// Recent queue-to-trunk times.
    pub(super) timings: Timings,
}

/// The lease role landers fence their trunk writes with.
pub const LANDER_LEASE: &str = "lander";

/// The lease role of the lander of `line`: one lease per line.
pub fn lander_lease(line: &Line) -> String {
    match line {
        Line::Trunk => LANDER_LEASE.to_string(),
        Line::Release(name) => format!("{LANDER_LEASE}:release/{name}"),
    }
}

impl Lander {
    /// A lander over `repo`, without regenerate drivers or a resolver.
    pub fn new(
        repo: RepoStore,
        derive: Arc<DeriveManager>,
        graph: Arc<CommitGraph>,
        hooks: Arc<dyn ReviewHooks>,
        verifier: Arc<dyn Verifier>,
        policy: LandPolicy,
    ) -> Self {
        let (events, _) = broadcast::channel(EVENT_CAPACITY);
        Lander {
            lease: LeaseHolder::new(repo.clone(), LANDER_LEASE),
            queue: LandQueue::new(repo.clone()),
            repo,
            derive,
            graph,
            hooks,
            verifier,
            line: Line::Trunk,
            regenerator: Arc::new(NoRegenerator),
            resolver: None,
            fixer: None,
            targets: None,
            touched: Default::default(),
            policy,
            events,
            rules: parking_lot::Mutex::new(None),
            thrash: Thrash::new(),
            timings: Timings::new(),
        }
    }

    /// Land on `line` rather than trunk, under that line's own lease.
    pub fn on_line(mut self, line: Line) -> Self {
        self.lease = LeaseHolder::new(self.repo.clone(), &lander_lease(&line));
        self.queue = LandQueue::new(self.repo.clone()).on_line(line.clone());
        self.line = line;
        self
    }

    /// What this lander lands on.
    pub fn line(&self) -> &Line {
        &self.line
    }

    /// Run `regenerate` merge drivers with `regenerator`.
    pub fn with_regenerator(mut self, regenerator: Arc<dyn Regenerator>) -> Self {
        self.regenerator = regenerator;
        self
    }

    /// Offer new trunk incidents to `fixer`.
    pub fn with_fixer(mut self, fixer: Arc<dyn FixDispatcher>) -> Self {
        self.fixer = Some(fixer);
        self
    }

    /// Treat requests touching a common build target as interacting.
    pub fn with_targets(mut self, targets: Arc<dyn TargetGraph>) -> Self {
        self.targets = Some(targets);
        self
    }

    /// Offer conflicts the drivers leave to `resolver` before their owner.
    pub fn with_resolver(mut self, resolver: Arc<dyn ConflictResolver>) -> Self {
        self.resolver = Some(resolver);
        self
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
        self.check_line(std::slice::from_ref(&request))?;
        let change_id = request.change_id;
        if let Some(ticket) = self.repo.active_land_request(&change_id).await? {
            return Ok(ticket);
        }
        self.admit_requests(std::slice::from_ref(&request)).await?;
        let (ticket, new) = self.queue.enqueue(&request).await?;
        if new {
            self.publish(LandEvent::Queued { ticket, change_id });
        }
        Ok(ticket)
    }

    /// Queue `requests` (bottom first) as one ordered group that lands
    /// whole or, in `Prefix` mode, as far as it can. Returns their tickets
    /// in order.
    pub async fn enqueue_group(
        &self,
        requests: Vec<LandRequest>,
        mode: GroupMode,
    ) -> Result<Vec<LandTicket>> {
        self.check_line(&requests)?;
        if requests.is_empty() || requests.len() > MAX_BATCH {
            return Err(crate::error::Error::GroupSize {
                size: requests.len(),
                limit: MAX_BATCH,
            });
        }
        for request in &requests {
            if let Some(ticket) = self.repo.active_land_request(&request.change_id).await? {
                return Err(crate::error::Error::AlreadyQueued {
                    change: request.change_id,
                    ticket,
                });
            }
        }
        self.admit_requests(&requests).await?;
        let tickets = self.repo.queue_land_group(&requests, mode).await?;
        for (ticket, request) in tickets.iter().zip(&requests) {
            self.publish(LandEvent::Queued {
                ticket: *ticket,
                change_id: request.change_id,
            });
        }
        Ok(tickets)
    }

    fn check_line(&self, requests: &[LandRequest]) -> Result<()> {
        match requests.iter().find(|r| r.line != self.line) {
            Some(r) => Err(crate::error::Error::Config(format!(
                "the lander of {} cannot land a request for {}",
                self.line, r.line
            ))),
            None => Ok(()),
        }
    }

    /// Refuse `requests` when the queue, their requester or their sponsor
    /// has no room for them; the emergency lane always has room.
    async fn admit_requests(&self, requests: &[LandRequest]) -> Result<()> {
        let Some(first) = requests.first() else {
            return Ok(());
        };
        if first.lane == rill_types::Lane::Emergency {
            return Ok(());
        }
        let adding = requests.len();
        let pending = self.queue.pending(self.policy.max_queued + 1).await?;
        if pending.len() + adding > self.policy.max_queued {
            let wait = self.timings.snapshot().interval * pending.len() as u32;
            return Err(crate::error::Error::QueueFull {
                pending: pending.len(),
                retry_after_secs: wait.as_secs().clamp(1, 600),
            });
        }
        let mine = pending
            .iter()
            .filter(|q| q.request.requester.id == first.requester.id)
            .count();
        if mine + adding > self.policy.max_in_flight {
            return Err(crate::error::Error::TooManyRequests {
                requester: first.requester.name.clone(),
                limit: self.policy.max_in_flight,
            });
        }
        if let Some(sponsor) = &first.sponsor {
            let sponsored = pending
                .iter()
                .filter(|q| q.request.sponsor.as_ref() == Some(sponsor))
                .count();
            if sponsored + adding > self.policy.max_sponsor_in_flight {
                return Err(crate::error::Error::TooManyRequests {
                    requester: sponsor.clone(),
                    limit: self.policy.max_sponsor_in_flight,
                });
            }
        }
        Ok(())
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
        let queued = keep_groups_together(serve_order(queued, &self.policy.sponsor_weights));
        Ok(self.sequence(queued, &lease).await)
    }

    /// Where `ticket` stands: its place in the serve order (0 = next) and
    /// when it should land at the recent rate. `None` once it is not
    /// queued (or is past the first sweep's worth of requests).
    pub async fn position(
        &self,
        ticket: LandTicket,
    ) -> Result<Option<(usize, std::time::Duration)>> {
        let queued = keep_groups_together(serve_order(
            self.queue.pending(SWEEP_LIMIT).await?,
            &self.policy.sponsor_weights,
        ));
        Ok(queued
            .iter()
            .position(|q| q.ticket == ticket)
            .map(|at| (at, self.timings.snapshot().wait(at))))
    }

    /// Recent landing rate and queue-to-trunk times.
    pub fn timings(&self) -> Eta {
        self.timings.snapshot()
    }

    /// Each domain's recent conflict rate and attempts, for metrics.
    pub fn conflict_rates(&self) -> std::collections::BTreeMap<String, (f64, u32)> {
        self.thrash.rates(std::time::Duration::from_secs(300))
    }

    /// Unfinished requests per share key (sponsor or requester).
    pub async fn queued_by_share(&self) -> Result<std::collections::BTreeMap<String, usize>> {
        let mut out = std::collections::BTreeMap::new();
        for q in self.queue.pending(SWEEP_LIMIT).await? {
            *out.entry(share_key(&q)).or_insert(0) += 1;
        }
        Ok(out)
    }

    /// Serve the queue until `shutdown` is cancelled: after each sweep,
    /// wait for a new request or `batch_window`, whichever comes first.
    pub async fn run_forever(self: Arc<Self>, shutdown: CancellationToken) {
        let mut last_retention = tokio::time::Instant::now();
        loop {
            let seen = self.repo.land_wake_count(&self.line).await.unwrap_or(0);
            if let Err(e) = self.run_once().await {
                tracing::error!(error = %e, "landing sweep failed");
            }
            if last_retention.elapsed() >= RETENTION_SWEEP {
                last_retention = tokio::time::Instant::now();
                self.sweep_finished().await;
            }
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = self.repo.wait_for_land_requests(&self.line, seen, self.policy.batch_window) => {}
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
