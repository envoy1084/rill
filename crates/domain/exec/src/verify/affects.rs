//! Whether a change could change the outcome of some steps: the post-land
//! verifier narrows a culprit search with it, and a guarded trunk holds
//! back landings with it.

use std::collections::BTreeSet;

use rill_core::diff::diff_trees;
use rill_types::TreeHash;

use super::plan::load_plan;
use super::run::PlanRunner;
use super::select::{select, Selection};
use crate::error::Result;

impl PlanRunner {
    /// Whether going from `base` to `root` touched the inputs of any of
    /// `steps` in `root`'s plan (or the plan itself). Yes when it cannot be
    /// told: no base, or a step the plan does not name.
    pub async fn could_affect(
        &self,
        root: TreeHash,
        base: Option<TreeHash>,
        steps: &BTreeSet<String>,
    ) -> Result<bool> {
        let Some(base) = base else {
            return Ok(true);
        };
        let changed: Vec<_> = diff_trees(&self.trees, Some(base), Some(root))
            .await?
            .into_iter()
            .map(|entry| entry.path)
            .collect();
        let Some(plan) = load_plan(&self.trees, root).await? else {
            return Ok(false);
        };
        let mut named = 0;
        for step in plan.steps.iter().filter(|s| steps.contains(&s.name)) {
            named += 1;
            if select(step, Some(&changed), None) == Selection::Run {
                return Ok(true);
            }
        }
        Ok(named < steps.len())
    }
}
