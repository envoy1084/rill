//! Each request's footprint, for placing it among the others.

use rill_core::ChangedPaths;

use super::lander::Lander;
use crate::error::Result;
use crate::queue::QueuedRequest;
use crate::schedule::{Footprint, LandingRules};

impl Lander {
    /// What `queued`'s version changed, under `rules`.
    pub(super) async fn footprint(
        &self,
        queued: &QueuedRequest,
        rules: &LandingRules,
    ) -> Result<Footprint> {
        let paths = self
            .derive
            .get::<ChangedPaths>(&queued.request.version)
            .await?;
        Ok(Footprint::new(paths, rules))
    }
}
