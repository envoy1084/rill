//! Finding the first red position of a range.

use std::sync::Arc;

use rill_types::TrunkSeq;

use super::reverts::schedule_auto_revert;
use super::worker::PostLandVerifier;
use crate::error::Result;
use crate::events::LandEvent;

/// Positions a bisection round splits its range into.
pub const BISECT_WAYS: u64 = 8;

impl PostLandVerifier {
    /// Find the first red position in `(green, red]`, where `green` is a
    /// known-passing position (or zero) and `red` a known-red one, and
    /// report it. Each round verifies [`BISECT_WAYS`]` - 1` evenly spaced
    /// positions at once, so a range of 64 takes two rounds instead of six.
    pub async fn bisect(self: &Arc<Self>, green: TrunkSeq, red: TrunkSeq) -> Result<TrunkSeq> {
        self.bisect_with(green, red, false).await
    }

    /// [`PostLandVerifier::bisect`], verifying probes with the whole plan
    /// when `full`, else only what changed since the range's green end.
    pub(super) async fn bisect_with(
        self: &Arc<Self>,
        green: TrunkSeq,
        red: TrunkSeq,
        full: bool,
    ) -> Result<TrunkSeq> {
        let (mut lo, mut hi) = (green.get(), red.get());
        while hi - lo > 1 {
            let points = probes(lo, hi);
            let since = (!full).then_some(TrunkSeq(lo));
            let results = futures::future::join_all(
                points.iter().map(|p| self.verify_seq(TrunkSeq(*p), since)),
            )
            .await;
            for (point, record) in points.iter().zip(results) {
                if record?.passed() {
                    lo = *point;
                } else {
                    hi = *point;
                    break;
                }
            }
        }
        let culprit = TrunkSeq(hi);
        self.set_culprit(culprit, culprit).await?;
        tracing::warn!(%culprit, "post-land verification found a culprit");
        self.lander.publish(LandEvent::Culprit { seq: culprit });
        if self.lander.policy().auto_revert {
            schedule_auto_revert(&self.lander, culprit).await?;
        }
        Ok(culprit)
    }
}

/// The positions strictly between `lo` and `hi` a bisection round checks,
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
