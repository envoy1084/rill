//! Build targets as conflict domains, from the verify plan's `[graph]`.

use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use rill_exec::PlanRunner;
use rill_jobs::Priority;
use rill_lander::TargetGraph;
use rill_types::TreeHash;

/// Asks the repository's plan runner which targets a change touches.
pub struct ExecTargetGraph {
    runner: Arc<PlanRunner>,
}

impl ExecTargetGraph {
    pub fn new(runner: Arc<PlanRunner>) -> Self {
        ExecTargetGraph { runner }
    }
}

#[async_trait]
impl TargetGraph for ExecTargetGraph {
    async fn touched(
        &self,
        root: TreeHash,
        base: Option<TreeHash>,
    ) -> rill_lander::Result<Option<BTreeSet<String>>> {
        // A graph that cannot be read leaves path domains: it must not
        // stop landing.
        match self
            .runner
            .touched_targets(root, base, Priority::High)
            .await
        {
            Ok(touched) => Ok(touched),
            Err(e) => {
                tracing::warn!(error = %e, "build graph unavailable; using path domains");
                Ok(None)
            }
        }
    }
}
