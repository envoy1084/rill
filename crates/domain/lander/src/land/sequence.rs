//! The sequencer: requests served in fair order, placed by what they
//! touch, speculated in parallel where they are independent, and landed
//! in groups of up to [`MAX_BATCH`] per transaction.
use std::collections::{BTreeSet, HashSet, VecDeque};

use rill_store::repo::Lease;
use rill_types::{LandStatus, LandTicket};

use super::lander::Lander;
use crate::error::Error;
use crate::events::LandEvent;
use crate::queue::QueuedRequest;
use crate::schedule::Footprint;

/// Landings committed together at most.
pub const MAX_BATCH: usize = 64;

/// What became of one window.
enum Window {
    /// Done; these requests were held back for a later batch.
    Done(Vec<QueuedRequest>),
    /// The lease was lost.
    Fenced,
}

impl Lander {
    /// Serve `queued` (in serve order), in batches of up to [`MAX_BATCH`].
    /// Returns how many requests were taken up.
    pub(super) async fn sequence(&self, queued: Vec<QueuedRequest>, lease: &Lease) -> usize {
        let count = queued.len();
        let mut marked = HashSet::new();
        let mut pending: VecDeque<QueuedRequest> = queued.into();
        while !pending.is_empty() {
            let take = pending.len().min(MAX_BATCH);
            let window: Vec<QueuedRequest> = pending.drain(..take).collect();
            match self.land_window(window, lease, &mut marked).await {
                Window::Fenced => break,
                // Every window lands or finishes at least its first request,
                // so what it held back is served in the next one.
                Window::Done(held) => {
                    for q in held.into_iter().rev() {
                        pending.push_front(q);
                    }
                }
            }
        }
        count
    }

    /// Land what can land of `window`.
    async fn land_window(
        &self,
        window: Vec<QueuedRequest>,
        lease: &Lease,
        marked: &mut HashSet<LandTicket>,
    ) -> Window {
        let mut live = window;
        let mut held = Vec::new();
        for attempt in 0..=self.policy.max_retries {
            let head = match self.repo.trunk_head().await {
                Ok(head) => head,
                Err(e) => {
                    tracing::warn!(error = %e, "reading the trunk head failed");
                    return Window::Done(held);
                }
            };
            let tip = head.map(|(_, h)| h);
            let rules = match self.landing_rules(tip).await {
                Ok(rules) => rules,
                Err(e) => {
                    tracing::warn!(error = %e, "reading the landing rules failed");
                    return Window::Done(held);
                }
            };
            let mut batch: Vec<(QueuedRequest, Footprint)> = Vec::new();
            for queued in &live {
                if marked.insert(queued.ticket) && !self.accept(queued).await {
                    continue;
                }
                match self.footprint(queued, &rules).await {
                    Ok(f) => batch.push((queued.clone(), f)),
                    Err(e) => self.fail(queued, e.to_string()).await,
                }
            }
            let (batch, later) = self.admit(batch, &rules.throttle);
            held.extend(later);
            let chain = self.speculate(tip, &batch).await;
            live = chain.iter().map(|c| c.queued.clone()).collect();
            if chain.is_empty() {
                return Window::Done(held);
            }
            match self.commit(head, chain, lease).await {
                Ok(()) => return Window::Done(held),
                Err(Error::Fenced) => return Window::Fenced,
                Err(e) if is_conflict(&e) => {
                    tracing::debug!(attempt, "trunk or a review moved; sequencing again");
                }
                Err(e) => {
                    for queued in &live {
                        self.fail(queued, e.to_string()).await;
                    }
                    return Window::Done(held);
                }
            }
        }
        for queued in &live {
            self.fail(
                queued,
                format!(
                    "trunk moved {} times while landing; giving up",
                    self.policy.max_retries + 1
                ),
            )
            .await;
        }
        Window::Done(held)
    }

    /// Split `batch` into what lands now and what waits: a configuration
    /// change lands alone, and a throttled domain gets one request.
    fn admit(
        &self,
        batch: Vec<(QueuedRequest, Footprint)>,
        throttle: &crate::schedule::ThrottleRules,
    ) -> (Vec<(QueuedRequest, Footprint)>, Vec<QueuedRequest>) {
        let throttled = self.thrash.throttled(throttle);
        let mut now = Vec::new();
        let mut later = Vec::new();
        let mut busy: BTreeSet<String> = BTreeSet::new();
        let mut cut = false;
        for (queued, footprint) in batch {
            // A configuration change ends the batch before it, or is all of
            // it: requests after it wait, so queue order is kept.
            cut |= footprint.exclusive && !now.is_empty();
            if cut {
                later.push(queued);
                continue;
            }
            if footprint.exclusive {
                now.push((queued, footprint));
                cut = true;
                continue;
            }
            let domains = footprint.domains();
            let hot: Vec<&String> = domains.intersection(&throttled).collect();
            if hot.iter().any(|d| busy.contains(*d)) {
                later.push(queued);
                continue;
            }
            busy.extend(hot.into_iter().cloned());
            now.push((queued, footprint));
        }
        if !throttled.is_empty() && !later.is_empty() {
            let rates = self
                .thrash
                .rates(std::time::Duration::from_secs(throttle.window_secs));
            for domain in &throttled {
                let rate = rates.get(domain).map_or(0.0, |r| r.0);
                tracing::info!(domain, rate, "domain throttled for swarm thrash");
                self.publish(LandEvent::Throttled {
                    domain: domain.clone(),
                    rate_permille: (rate * 1000.0) as u32,
                });
            }
        }
        (now, later)
    }

    /// Take a request up: review must still let the lander have it.
    async fn accept(&self, queued: &QueuedRequest) -> bool {
        let change_id = queued.request.change_id;
        match self.hooks.mark_landing(change_id).await {
            Ok(()) => true,
            Err(e) => {
                let reason = format!("change is no longer landable: {e}");
                tracing::warn!(%change_id, ticket = %queued.ticket, reason, "landing refused");
                self.set_status(
                    queued,
                    LandStatus::Failed {
                        reason: reason.clone(),
                    },
                )
                .await;
                self.publish(LandEvent::Failed { change_id, reason });
                false
            }
        }
    }
}

fn is_conflict(e: &Error) -> bool {
    matches!(e, Error::Store(s) if s.is_conflict())
}
