//! Finding the first red position of a range.

use std::collections::BTreeSet;
use std::sync::Arc;

use rill_core::RootTree;
use rill_types::TrunkSeq;

use super::record::{record_at, VerificationRecord};
use super::reverts::schedule_auto_revert;
use super::worker::PostLandVerifier;
use crate::error::Result;
use crate::events::LandEvent;
use crate::traits::Failure;

/// Positions a bisection round splits its range into.
pub const BISECT_WAYS: u64 = 8;

/// What a search looks for.
pub(super) struct Search<'a> {
    /// Verify probes with the whole plan rather than what changed since
    /// the range's passing end.
    pub full: bool,
    /// Only these steps failing makes a probe red: finding a new break on
    /// a trunk that is already red for other reasons.
    pub focus: Option<&'a BTreeSet<String>>,
    /// What failed at the red end, to narrow the range and to attribute.
    pub failures: &'a [Failure],
    pub details: &'a str,
}

impl PostLandVerifier {
    /// Find the first red position in `(green, red]`, where `green` is a
    /// known-passing position (or zero) and `red` a known-red one, and
    /// report it. Each round verifies [`BISECT_WAYS`]` - 1` evenly spaced
    /// positions at once, so a range of 64 takes two rounds instead of six.
    pub async fn bisect(self: &Arc<Self>, green: TrunkSeq, red: TrunkSeq) -> Result<TrunkSeq> {
        let search = Search {
            full: false,
            focus: None,
            failures: &[],
            details: "",
        };
        self.bisect_with(green, red, &search).await
    }

    /// [`PostLandVerifier::bisect`] for `search`. Only positions that could
    /// affect the failing steps can have turned them red, so only those
    /// (and the red end) are searched; the culprit gets an incident.
    pub(super) async fn bisect_with(
        self: &Arc<Self>,
        green: TrunkSeq,
        red: TrunkSeq,
        search: &Search<'_>,
    ) -> Result<TrunkSeq> {
        let steps: BTreeSet<String> = match search.focus {
            Some(focus) => focus.clone(),
            None => search.failures.iter().map(|f| f.step.clone()).collect(),
        };
        let candidates = self.candidates(green, red, &steps).await?;
        // `lo` and `hi` index `candidates` from 1; 0 is the passing end.
        let (mut lo, mut hi) = (0u64, candidates.len() as u64);
        while hi - lo > 1 {
            let points = probes(lo, hi);
            let since = |lo: u64| match lo {
                0 => green,
                i => candidates[i as usize - 1],
            };
            let base = (!search.full).then_some(since(lo));
            let results = futures::future::join_all(
                points
                    .iter()
                    .map(|p| self.verify_seq(candidates[*p as usize - 1], base)),
            )
            .await;
            for (point, record) in points.iter().zip(results) {
                if is_red(&record?, search.focus) {
                    hi = *point;
                    break;
                }
                lo = *point;
            }
        }
        let culprit = candidates[hi as usize - 1];
        self.set_culprit(culprit, culprit).await?;
        tracing::warn!(%culprit, searched = candidates.len(), "post-land verification found a culprit");
        self.lander.publish(LandEvent::Culprit { seq: culprit });
        let own = record_at(self.lander.repo(), culprit)
            .await?
            .filter(|r| !r.inferred && !r.failures.is_empty());
        let (failures, details) = match &own {
            Some(record) => (record.failures.as_slice(), record.details.as_str()),
            None => (search.failures, search.details),
        };
        // A search for a new break attributes only the new failures; the
        // rest are the earlier incidents'.
        let failures: Vec<Failure> = failures
            .iter()
            .filter(|f| search.focus.is_none_or(|steps| steps.contains(&f.step)))
            .cloned()
            .collect();
        self.lander
            .open_incident(culprit, &failures, details)
            .await?;
        if self.lander.policy().auto_revert {
            schedule_auto_revert(&self.lander, culprit).await?;
        }
        Ok(culprit)
    }

    /// The positions of `(green, red]` whose landing could affect `steps`,
    /// ending with `red`; every position when that cannot be narrowed.
    async fn candidates(
        &self,
        green: TrunkSeq,
        red: TrunkSeq,
        steps: &BTreeSet<String>,
    ) -> Result<Vec<TrunkSeq>> {
        let repo = self.lander.repo();
        let range = repo.trunk_range(green, red).await?;
        let all: Vec<TrunkSeq> = range.iter().map(|(seq, _)| *seq).collect();
        if steps.is_empty() || all.len() <= 1 {
            return Ok(all);
        }
        let derive = self.lander.derive();
        let verifier = self.lander.verifier();
        let checks = range.iter().map(|(seq, version)| async move {
            let cv = derive.change(version).await?;
            let root = derive.get::<RootTree>(version).await?;
            let base = match cv.first_parent() {
                Some(parent) => Some(derive.get::<RootTree>(&parent).await?),
                None => None,
            };
            let affects = verifier.could_affect(root, base, steps).await?;
            Ok::<_, crate::error::Error>((*seq, affects))
        });
        let mut out: Vec<TrunkSeq> = Vec::new();
        for result in futures::future::join_all(checks).await {
            let (seq, affects) = result?;
            if affects && seq != red {
                out.push(seq);
            }
        }
        out.push(red);
        Ok(out)
    }
}

/// Whether `record` is red for a search: any failure, or with a focus, a
/// failure of one of its steps.
fn is_red(record: &VerificationRecord, focus: Option<&BTreeSet<String>>) -> bool {
    if record.passed() {
        return false;
    }
    match focus {
        None => true,
        Some(steps) => record.failures.iter().any(|f| steps.contains(&f.step)),
    }
}

/// The points strictly between `lo` and `hi` a bisection round checks,
/// ascending.
fn probes(lo: u64, hi: u64) -> Vec<u64> {
    let span = hi - lo;
    let mut points: Vec<u64> = (1..BISECT_WAYS)
        .map(|i| lo + span * i / BISECT_WAYS)
        .filter(|p| *p > lo && *p < hi)
        .collect();
    points.dedup();
    points
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_split_evenly_and_shrink_to_one() {
        assert_eq!(probes(0, 64), vec![8, 16, 24, 32, 40, 48, 56]);
        assert_eq!(probes(32, 40), vec![33, 34, 35, 36, 37, 38, 39]);
        assert_eq!(probes(4, 6), vec![5]);
        assert!(probes(4, 5).is_empty());
    }
}
