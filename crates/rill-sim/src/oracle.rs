//! The invariants a run must keep, checked against what the actors
//! observed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use rill_lander::LandTicket;
use rill_store::RepoStore;
use rill_types::{ChangeId, ChangeVersionHash, ReviewStatus, TrunkSeq};

/// Which invariant broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Invariant {
    /// Trunk positions `1..=head` must all exist and map back to their seq.
    TrunkDense,
    /// A trunk position never changes once observed.
    TrunkStable,
    /// A change lands at most once.
    LandsOnce,
    /// An abandoned change never lands.
    NoLandingAfterAbandon,
    /// A version whose write was acknowledged stays readable.
    AcknowledgedWritesSurvive,
    /// Rebase fails only on storage errors, never on content.
    RebaseNeverFailsOnContent,
    /// Every land request reaches a final, queryable outcome.
    LandRequestsResolve,
    /// A landed change's review record says so.
    ReviewMatchesTrunk,
    /// The event log holds exactly one event per stored version and per
    /// trunk position.
    EventsMatchState,
    /// Every job finishes once faults stop, including those whose worker
    /// died holding them.
    JobsFinish,
    /// A lease holder that was replaced cannot land.
    DeposedHolderFenced,
}

/// One broken invariant with context.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub invariant: Invariant,
    pub detail: String,
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.invariant, self.detail)
    }
}

/// What the actors know about the system, independent of its internals.
#[derive(Debug, Default)]
pub struct Oracle {
    trunk: Vec<ChangeVersionHash>,
    acknowledged: Vec<ChangeVersionHash>,
    requested: BTreeMap<ChangeId, LandTicket>,
    abandoned: BTreeSet<ChangeId>,
    /// Jobs whose worker died holding them.
    pub killed_jobs: BTreeSet<rill_types::JobId>,
    /// Writes a deposed lease holder tried, all of which must fail.
    pub fenced_attempts: usize,
    pub violations: Vec<Violation>,
}

impl Oracle {
    pub fn acknowledged(&mut self, version: ChangeVersionHash) {
        self.acknowledged.push(version);
    }

    pub fn requested(&mut self, change: ChangeId, ticket: LandTicket) {
        self.requested.insert(change, ticket);
    }

    pub fn abandoned(&mut self, change: ChangeId) {
        self.abandoned.insert(change);
    }

    /// A version some write acknowledged, picked by `roll`.
    pub fn acknowledged_at(&self, roll: usize) -> Option<ChangeVersionHash> {
        (!self.acknowledged.is_empty()).then(|| self.acknowledged[roll % self.acknowledged.len()])
    }

    /// The trunk observed last.
    pub fn trunk(&self) -> &[ChangeVersionHash] {
        &self.trunk
    }

    pub fn ticket(&self, change: &ChangeId) -> Option<LandTicket> {
        self.requested.get(change).copied()
    }

    pub fn is_abandoned(&self, change: &ChangeId) -> bool {
        self.abandoned.contains(change)
    }

    /// Positions observed on trunk so far.
    pub fn trunk_len(&self) -> usize {
        self.trunk.len()
    }

    pub fn violate(&mut self, invariant: Invariant, detail: impl Into<String>) {
        self.violations.push(Violation {
            invariant,
            detail: detail.into(),
        });
    }

    /// Read trunk and check density, stability, single landing and the
    /// abandon rule. Store errors skip the check; they are faults, not
    /// violations.
    pub async fn observe_trunk(&mut self, repo: &RepoStore) {
        let Ok(head) = repo.trunk_head().await else {
            return;
        };
        let head = head.map(|(seq, _)| seq.get() as usize).unwrap_or(0);
        if head < self.trunk.len() {
            self.violate(
                Invariant::TrunkStable,
                format!("trunk shrank from {} to {head}", self.trunk.len()),
            );
            return;
        }
        let mut seen: Vec<ChangeVersionHash> = Vec::with_capacity(head);
        for seq in 1..=head as u64 {
            match repo.trunk_at(TrunkSeq(seq)).await {
                Ok(Some(hash)) => seen.push(hash),
                Ok(None) => {
                    self.violate(Invariant::TrunkDense, format!("trunk@{seq} missing"));
                    return;
                }
                Err(_) => return,
            }
        }
        let changed: Vec<usize> = self
            .trunk
            .iter()
            .zip(&seen)
            .enumerate()
            .filter(|(_, (old, new))| old != new)
            .map(|(i, _)| i + 1)
            .collect();
        for seq in changed {
            self.violate(Invariant::TrunkStable, format!("trunk@{seq} changed"));
        }
        let mut changes = BTreeSet::new();
        for (i, hash) in seen.iter().enumerate() {
            let Ok(Some(cv)) = repo.get_change_version(hash).await else {
                continue;
            };
            if !changes.insert(cv.change_id) && i >= self.trunk.len() {
                self.violate(
                    Invariant::LandsOnce,
                    format!("{} landed again at trunk@{}", cv.change_id, i + 1),
                );
            }
            if i >= self.trunk.len() && self.abandoned.contains(&cv.change_id) {
                self.violate(
                    Invariant::NoLandingAfterAbandon,
                    format!("abandoned {} landed at trunk@{}", cv.change_id, i + 1),
                );
            }
        }
        self.trunk = seen;
    }

    /// Every acknowledged version must still be readable.
    pub async fn check_durability(&mut self, repo: &RepoStore) {
        for hash in self.acknowledged.clone() {
            if let Ok(None) = repo.get_change_version(&hash).await {
                self.violate(
                    Invariant::AcknowledgedWritesSurvive,
                    format!("version {} lost", hash.short()),
                );
            }
        }
    }

    /// After the run settles: every requested landing has a final outcome
    /// and every landed change's review says it landed.
    pub async fn check_settled(
        &mut self,
        repo: &RepoStore,
        status: impl Fn(LandTicket) -> Option<rill_lander::LandStatus>,
    ) {
        let landed: BTreeSet<ChangeId> = {
            let mut out = BTreeSet::new();
            for hash in &self.trunk {
                if let Ok(Some(cv)) = repo.get_change_version(hash).await {
                    out.insert(cv.change_id);
                }
            }
            out
        };
        for (change, ticket) in self.requested.clone() {
            match status(ticket) {
                Some(s) if s.is_terminal() => {}
                Some(s) => self.violate(
                    Invariant::LandRequestsResolve,
                    format!("{change} still {s:?}"),
                ),
                None if landed.contains(&change) => {}
                None => self.violate(
                    Invariant::LandRequestsResolve,
                    format!("land request for {change} lost"),
                ),
            }
        }
        for change in &landed {
            if let Ok(Some(state)) = repo.get_review(change).await {
                if state.status != ReviewStatus::Landed {
                    self.violate(
                        Invariant::ReviewMatchesTrunk,
                        format!("{change} is on trunk but its review is {:?}", state.status),
                    );
                }
            }
        }
    }
}
