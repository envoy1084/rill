//! Pre-land and post-land verification through remote execution.

use async_trait::async_trait;
use rill_exec::{PlanRunner, Target};
use rill_jobs::Priority;
use rill_lander::{VerificationOutcome, Verifier, VerifyRequest, VerifyStage};

/// Runs a tree's `.rill/verify.toml` through the scheduler and reports
/// the outcome to the lander.
pub struct ExecVerifier {
    runner: PlanRunner,
}

impl ExecVerifier {
    /// A verifier running plans with `runner`.
    pub fn new(runner: PlanRunner) -> Self {
        ExecVerifier { runner }
    }
}

#[async_trait]
impl Verifier for ExecVerifier {
    async fn verify(&self, request: VerifyRequest) -> rill_lander::Result<VerificationOutcome> {
        let target = Target {
            root: request.root,
            base: request.base,
            risk: request.risk,
            priority: match request.stage {
                VerifyStage::PreLand => Priority::High,
                VerifyStage::PostLand => Priority::Normal,
            },
        };
        let outcome = match self.runner.verify(&target).await {
            Ok(outcome) => outcome,
            // A plan that cannot run as written fails the change that
            // carries it rather than the verifier.
            Err(rill_exec::Error::Config { path, reason }) => {
                return Ok(VerificationOutcome::red(format!(
                    "invalid {path}: {reason}"
                )));
            }
            Err(e) => return Err(rill_lander::Error::Verifier(e.to_string())),
        };
        let mut details = outcome.summary();
        if let Some(excerpt) = &outcome.excerpt {
            details.push('\n');
            details.push_str(excerpt);
        }
        tracing::info!(version = %request.version, green = outcome.green, summary = %outcome.summary(), "verification");
        Ok(if outcome.green {
            VerificationOutcome::green(details)
        } else {
            VerificationOutcome::red(details)
        })
    }
}
