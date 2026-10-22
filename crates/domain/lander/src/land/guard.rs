//! What a red trunk holds back: when frozen, everything but the emergency
//! lane; when guarded, landings that could affect a failing step, unless
//! they fix an open incident.

use std::collections::BTreeSet;

use rill_core::RootTree;
use rill_types::{HealthState, Lane, TrunkHealth};

use super::lander::Lander;
use crate::healing::fixes_of;
use crate::queue::QueuedRequest;
use crate::schedule::Footprint;

impl Lander {
    /// Split `batch` into what may land under trunk's `health` and what
    /// waits.
    pub(super) async fn guard(
        &self,
        batch: Vec<(QueuedRequest, Footprint)>,
        health: &TrunkHealth,
    ) -> (Vec<(QueuedRequest, Footprint)>, Vec<QueuedRequest>) {
        let guarded = health.state == HealthState::Guarded;
        if !health.frozen && !guarded {
            return (batch, Vec::new());
        }
        let failing: BTreeSet<String> = health.failing_steps.iter().cloned().collect();
        let mut now = Vec::with_capacity(batch.len());
        let mut later = Vec::new();
        for (queued, footprint) in batch {
            let emergency = queued.request.lane == Lane::Emergency;
            let wait = if health.frozen {
                !emergency
            } else {
                !emergency
                    && !self.fixes_open(&queued, health).await
                    && self.touches(&queued, &failing).await
            };
            match wait {
                true => later.push(queued),
                false => now.push((queued, footprint)),
            }
        }
        if !later.is_empty() {
            tracing::info!(
                held = later.len(),
                frozen = health.frozen,
                "trunk is guarded: holding landings that touch what fails"
            );
        }
        (now, later)
    }

    /// Whether `queued` names an open incident's culprit as fixed.
    async fn fixes_open(&self, queued: &QueuedRequest, health: &TrunkHealth) -> bool {
        match self.derive.change(&queued.request.version).await {
            Ok(cv) => fixes_of(&cv).iter().any(|seq| health.open.contains(seq)),
            Err(_) => false,
        }
    }

    /// Whether `queued`'s change could affect one of `steps`; yes when it
    /// cannot be told.
    async fn touches(&self, queued: &QueuedRequest, steps: &BTreeSet<String>) -> bool {
        let version = queued.request.version;
        let answer = async {
            let cv = self.derive.change(&version).await?;
            let root = self.derive.get::<RootTree>(&version).await?;
            let base = match cv.first_parent() {
                Some(parent) => Some(self.derive.get::<RootTree>(&parent).await?),
                None => None,
            };
            self.verifier.could_affect(root, base, steps).await
        };
        answer.await.unwrap_or(true)
    }
}
