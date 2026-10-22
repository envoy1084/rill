//! Running a plan's graph query on a tree.

use rill_jobs::Priority;
use rill_types::TreeHash;

use super::hashes::{parse_target_hashes, TargetHashes};
use super::spec::GraphSpec;
use crate::error::{Error, Result};
use crate::remote::BlobDigest;
use crate::verify::compile::compile;
use crate::verify::outcome::StepResult;
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
}
