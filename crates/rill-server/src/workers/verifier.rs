//! Pre-land verification through `rill-exec`.

use std::sync::Arc;

use async_trait::async_trait;
use rill_exec::{verify_tree, Runner};
use rill_lander::{VerificationOutcome, Verifier};
use rill_types::{ChangeVersionHash, TreeHash};

/// Runs the tree's `.rill/verify.toml` plan in the configured sandbox and
/// reports the outcome to the lander.
pub struct ExecVerifier {
    runner: Arc<Runner>,
}

impl ExecVerifier {
    /// A verifier executing through `runner`.
    pub fn new(runner: Arc<Runner>) -> Self {
        ExecVerifier { runner }
    }
}

#[async_trait]
impl Verifier for ExecVerifier {
    async fn verify(
        &self,
        root: TreeHash,
        version: ChangeVersionHash,
    ) -> rill_lander::Result<VerificationOutcome> {
        let outcome = verify_tree(&self.runner, root)
            .await
            .map_err(|e| rill_lander::Error::Verifier(e.to_string()))?;
        let details = match outcome.first_failure() {
            Some(step) => format!(
                "step {:?} failed with exit code {} ({} step(s) ran)",
                step.name,
                step.result.exit_code,
                outcome.steps.len()
            ),
            None => format!("{} step(s) passed", outcome.steps.len()),
        };
        tracing::info!(%version, green = outcome.green, details, "pre-land verification");
        Ok(if outcome.green {
            VerificationOutcome::green(details)
        } else {
            VerificationOutcome::red(details)
        })
    }
}
