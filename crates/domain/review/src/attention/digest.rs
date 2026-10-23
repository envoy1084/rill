//! A day's summary of the work under an intent.

use rill_policy::intents::day_of;
use rill_types::{Intent, ReviewStatus, TrunkSeq};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::service::ReviewService;

/// What happened under an intent on one UTC day.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntentDigest {
    pub day: String,
    /// Changes that landed that day.
    pub landed: u32,
    /// Of those, ones since reverted or blamed for a red trunk.
    pub reverted: u32,
    /// Of those, ones a reviewer voted against at some point.
    pub flagged: u32,
    /// Of those, ones sampled for a human audit.
    pub sampled: u32,
    /// Changes still in review.
    pub open: u32,
}

impl ReviewService {
    /// The digest of `intent` for `day` (`YYYY-MM-DD`).
    pub async fn intent_digest(&self, intent: &Intent, day: &str) -> Result<IntentDigest> {
        let mut d = IntentDigest {
            day: day.to_string(),
            ..IntentDigest::default()
        };
        for change in &intent.changes {
            let Some(state) = self.repo.get_review(change).await? else {
                continue;
            };
            if matches!(state.status, ReviewStatus::Open | ReviewStatus::Landable) {
                d.open += 1;
                continue;
            }
            let Some(seq) = state.landed_seq else {
                continue;
            };
            let Some(landing) = self.repo.landing(seq).await? else {
                continue;
            };
            if day_of(landing.landed_at) != day {
                continue;
            }
            d.landed += 1;
            if self.repo.get_misc(&escaped_key(seq)).await?.is_some() {
                d.reverted += 1;
            }
            if self.review_runs(change).await?.iter().any(|r| r.vote < 0) {
                d.flagged += 1;
            }
            if self.audit(seq).await?.is_some() {
                d.sampled += 1;
            }
        }
        Ok(d)
    }
}

/// The marker calibration leaves on a landing counted as an escape.
pub fn escaped_key(seq: TrunkSeq) -> String {
    format!("escaped:{}", seq.0)
}
