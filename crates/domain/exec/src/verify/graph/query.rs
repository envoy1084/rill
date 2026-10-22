//! Running a plan's graph query on a tree.

use rill_jobs::Priority;
use rill_types::TreeHash;

use std::collections::BTreeSet;

use super::affected::touched_targets;
use super::hashes::{parse_target_hashes, TargetHashes};
use super::spec::GraphSpec;
use crate::error::{Error, Result};
use crate::remote::BlobDigest;
use crate::verify::compile::compile;
use crate::verify::outcome::StepResult;
use crate::verify::plan::load_plan;
use crate::verify::run::PlanRunner;

impl PlanRunner {
    /// The target hashes of `tree`, or the query's failed result.
    pub(crate) async fn target_hashes(
        &self,
        spec: &GraphSpec,
        tree: TreeHash,
        priority: Priority,
    ) -> Result<std::result::Result<TargetHashes, StepResult>> {
        let mut actions = compile(
            &self.space,
            &self.trees,
            tree,
            &spec.step,
            &self.defaults,
            None,
        )
        .await?;
        let action = actions
            .pop()
            .ok_or_else(|| Error::InvalidAction("a graph query compiled to nothing".into()))?;
        let (step, result) = self.submit(action, priority, None).await?;
        if !step.status.is_ok() {
            return Ok(Err(step));
        }
        let output = result
            .as_ref()
            .and_then(|r| r.output_files.iter().find(|f| f.path == spec.output))
            .and_then(|f| f.digest.as_ref())
            .ok_or_else(|| Error::Config {
                path: crate::verify::PLAN_PATH.into(),
                reason: format!("the graph query did not write {:?}", spec.output),
            })?;
        let digest = BlobDigest::parse(output)?;
        let bytes = self
            .space
            .read(&digest)
            .await?
            .ok_or_else(|| Error::MissingBlob(digest.to_string()))?;
        Ok(Ok(parse_target_hashes(&bytes)?))
    }

    /// The targets a change from `base` to `root` touches, under the
    /// build graph `root`'s plan declares: changed, added or removed.
    /// `None` when the plan declares no graph or the query fails, so the
    /// caller falls back to paths.
    pub async fn touched_targets(
        &self,
        root: TreeHash,
        base: Option<TreeHash>,
        priority: Priority,
    ) -> Result<Option<BTreeSet<String>>> {
        let Some(spec) = load_plan(&self.trees, root).await?.and_then(|p| p.graph) else {
            return Ok(None);
        };
        let after = match self.target_hashes(&spec, root, priority).await? {
            Ok(hashes) => hashes,
            Err(failed) => {
                tracing::warn!(
                    status = failed.status.describe(),
                    "build graph query failed"
                );
                return Ok(None);
            }
        };
        let before = match base {
            Some(base) => match self.target_hashes(&spec, base, priority).await? {
                Ok(hashes) => Some(hashes),
                Err(_) => return Ok(None),
            },
            None => None,
        };
        Ok(Some(touched_targets(before.as_ref(), &after)))
    }
}
