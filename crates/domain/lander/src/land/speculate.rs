//! Speculation: requests that touch nothing in common are rebased onto
//! the trunk head in parallel; a request that overlaps earlier ones is
//! rebased on top of them. At commit the batch is lined up: each request
//! is re-parented onto the one before it, which for disjoint changes is a
//! pure tree operation that cannot conflict.

use std::collections::HashMap;

use futures::future::join_all;
use rill_types::{ChangeVersionHash, GroupMode, LandGroup, LandTicket};

use super::head_config::HeadRules;
use super::lander::Lander;
use super::prepare::{Candidate, Prepared};
use crate::error::Result;
use crate::queue::QueuedRequest;
use crate::schedule::{group_failure, group_of, Footprint, Plan, Slot};

impl Lander {
    /// Prepare `batch` (in commit order) as a chain on `head`. Returns the
    /// chain and the requests held back for a later batch.
    ///
    /// Ordered groups land whole (`All`) or up to their first member that
    /// cannot (`Prefix`); members above one that failed are not prepared.
    pub(super) async fn speculate(
        &self,
        head: Option<ChangeVersionHash>,
        batch: &[(QueuedRequest, Footprint)],
        rules: &HeadRules,
    ) -> (Vec<Candidate>, Vec<QueuedRequest>) {
        let footprints: Vec<Footprint> = batch.iter().map(|(_, f)| f.clone()).collect();
        let plan = Plan::new(&footprints);
        let n = plan.len();
        let mut ready: Vec<Option<Candidate>> = (0..n).map(|_| None).collect();
        let mut alive = vec![true; n];
        let mut placed = vec![Slot::Head; n];
        let mut broken = Broken::default();
        for wave in plan.waves() {
            let mut jobs = Vec::new();
            for i in wave {
                if let Some(group) = group_of(&batch[i].0).filter(|g| broken.dooms(g)) {
                    alive[i] = false;
                    self.fail(&batch[i].0, group_failure(group.mode).into())
                        .await;
                    continue;
                }
                let slot = plan.slot(i, &alive, &placed);
                placed[i] = slot.clone();
                let base = match slot {
                    Slot::Head => head,
                    Slot::After(j) => ready[j].as_ref().map(|c| c.version),
                    Slot::Linear => continue,
                };
                jobs.push(async move { (i, self.prepare(&batch[i].0, base, rules).await) });
            }
            for (i, outcome) in join_all(jobs).await {
                let conflicted = matches!(outcome, Ok(Prepared::Conflicted));
                self.thrash.record(&batch[i].1.domains(), conflicted);
                match outcome {
                    Ok(Prepared::Ready(candidate)) => ready[i] = Some(*candidate),
                    Ok(_) => alive[i] = false,
                    Err(e) => {
                        alive[i] = false;
                        self.fail(&batch[i].0, e.to_string()).await;
                    }
                }
                if let Some(group) = group_of(&batch[i].0).filter(|_| !alive[i]) {
                    broken.lose(group);
                }
            }
        }

        let mut chain: Vec<Candidate> = Vec::new();
        let mut held = Vec::new();
        let mut tip = head;
        for i in 0..n {
            if !alive[i] {
                continue;
            }
            let group = group_of(&batch[i].0);
            if let Some(group) = group.filter(|g| broken.dooms(g)) {
                self.fail(&batch[i].0, group_failure(group.mode).into())
                    .await;
                continue;
            }
            let lined = match (placed[i] == Slot::Linear, ready[i].take()) {
                (true, _) => match self.prepare(&batch[i].0, tip, rules).await {
                    Ok(Prepared::Ready(c)) => Some(*c),
                    Ok(_) => None,
                    Err(e) => {
                        self.fail(&batch[i].0, e.to_string()).await;
                        None
                    }
                },
                (false, Some(c)) => match self.reparent(c, tip, rules).await {
                    Ok(c) => c,
                    Err(e) => {
                        self.fail(&batch[i].0, e.to_string()).await;
                        None
                    }
                },
                (false, None) => None,
            };
            match (lined, group) {
                (Some(c), _) => {
                    tip = Some(c.version);
                    chain.push(c);
                }
                (None, Some(group)) => {
                    broken.lose(group);
                    if group.mode == GroupMode::All {
                        let dropped = self.drop_group(&mut chain, group.id()).await;
                        held.extend(dropped);
                        tip = chain.last().map(|c| c.version).or(head);
                    }
                }
                (None, None) => {}
            }
        }
        (chain, held)
    }

    /// Take group `id`'s members, and everything lined up after the first
    /// of them, off `chain`: the members fail, the rest is returned to be
    /// served again.
    async fn drop_group(&self, chain: &mut Vec<Candidate>, id: LandTicket) -> Vec<QueuedRequest> {
        let in_group = |c: &Candidate| group_of(&c.queued).is_some_and(|g| g.id() == id);
        let Some(first) = chain.iter().position(in_group) else {
            return Vec::new();
        };
        let mut held = Vec::new();
        for candidate in chain.drain(first..).collect::<Vec<_>>() {
            if in_group(&candidate) {
                self.fail(&candidate.queued, group_failure(GroupMode::All).into())
                    .await;
            } else {
                held.push(candidate.queued);
            }
        }
        held
    }

    /// `candidate` on `tip`. It was speculated against an earlier state
    /// holding everything it overlaps, so this only adds disjoint changes.
    async fn reparent(
        &self,
        candidate: Candidate,
        tip: Option<ChangeVersionHash>,
        rules: &HeadRules,
    ) -> Result<Option<Candidate>> {
        let Some(tip) = tip else {
            return Ok(Some(candidate));
        };
        if candidate.base == Some(tip) || candidate.cv.first_parent() == Some(tip) {
            return Ok(Some(candidate));
        }
        let Some(rebased) = self
            .rebase_through_ladder(&candidate.queued, &candidate.version, &tip, rules)
            .await?
        else {
            return Ok(None);
        };
        let version = self.repo.put_change_version(&rebased).await?;
        self.derive.ensure_land_critical(&version).await?;
        Ok(Some(Candidate {
            version,
            cv: rebased,
            base: Some(tip),
            ..candidate
        }))
    }
}

/// Groups with a member that will not land, by the lowest such member.
#[derive(Default)]
struct Broken(HashMap<LandTicket, u32>);

impl Broken {
    /// Record that `group`'s member will not land.
    fn lose(&mut self, group: &LandGroup) {
        let at = self.0.entry(group.id()).or_insert(group.index);
        *at = (*at).min(group.index);
    }

    /// Whether `group`'s member must not land: its group lands whole, or
    /// a member below it will not land.
    fn dooms(&self, group: &LandGroup) -> bool {
        self.0
            .get(&group.id())
            .is_some_and(|at| group.mode == GroupMode::All || group.index > *at)
    }
}
