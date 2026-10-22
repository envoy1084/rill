//! Land requests and a policy that batches quickly.

use std::time::Duration;

use rill_lander::{LandPolicy, LandRequest};
use rill_types::{ChangeVersion, Principal, RiskClass};

pub fn request(cv: &ChangeVersion, risk: RiskClass) -> LandRequest {
    LandRequest::new(
        cv.change_id,
        cv.version_hash(),
        Principal::human("alice"),
        risk,
    )
}

pub fn fast_policy() -> LandPolicy {
    LandPolicy {
        batch_window: Duration::from_millis(10),
        ..LandPolicy::default()
    }
}
