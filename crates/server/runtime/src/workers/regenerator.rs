//! `regenerate` merge drivers through remote execution.

use std::sync::Arc;

use async_trait::async_trait;
use rill_exec::verify::Regeneration;
use rill_exec::PlanRunner;
use rill_lander::{RegenerateRequest, Regenerated, Regenerator};

/// Runs a driver's command with the repository's plan runner, so it gets
/// the same workers, platform defaults and cache as verification.
pub struct ExecRegenerator {
    runner: Arc<PlanRunner>,
}

impl ExecRegenerator {
    pub fn new(runner: Arc<PlanRunner>) -> Self {
        ExecRegenerator { runner }
    }
}

#[async_trait]
impl Regenerator for ExecRegenerator {
    async fn regenerate(&self, request: RegenerateRequest) -> rill_lander::Result<Regenerated> {
        let job = Regeneration {
            name: request.driver,
            command: request.command,
            inputs: request.inputs,
            outputs: request.outputs,
            env: request.env,
            platform: request.platform,
            timeout: request.timeout,
        };
        match self.runner.regenerate(request.root, &job).await {
            Ok(Ok(files)) => Ok(Regenerated::Files(files)),
            Ok(Err(why)) => Ok(Regenerated::Failed(why)),
            Err(rill_exec::Error::Config { path, reason }) => {
                Ok(Regenerated::Failed(format!("invalid {path}: {reason}")))
            }
            Err(e) => Err(rill_lander::Error::Verifier(e.to_string())),
        }
    }
}
