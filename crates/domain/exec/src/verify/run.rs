//! Verifying a tree: select the steps a change needs, compile them to
//! actions, and run them through the scheduler.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use futures::stream::{FuturesUnordered, StreamExt};
use prost::Message;
use rill_core::diff::diff_trees;
use rill_core::TreeOps;
use rill_jobs::Priority;
use rill_reapi_proto::reapi::ExecuteResponse;
use rill_types::{RiskClass, TreeHash};

use super::compile::{compile, CompiledAction};
use super::outcome::{StepResult, StepStatus, VerificationOutcome};
use super::plan::load_plan;
use super::select::{select, Selection};
use crate::error::{Error, Result};
use crate::remote::{BlobDigest, ExecSpace, Scheduler, Stage, Submission};

/// How long one wait for an operation lasts before it is read again.
const WAIT: Duration = Duration::from_secs(30);
/// Bytes of a failure's output kept in the outcome.
const EXCERPT_BYTES: u64 = 4096;

/// A tree to verify.
#[derive(Debug, Clone)]
pub struct Target {
    pub root: TreeHash,
    /// The tree the change was made against. Steps whose inputs did not
    /// change since it are not run; `None` runs every step.
    pub base: Option<TreeHash>,
    /// The change's risk class; `None` runs every step whatever it gates.
    pub risk: Option<RiskClass>,
    pub priority: Priority,
}

/// Runs a repository's plans in its org's execution space.
pub struct PlanRunner {
    scheduler: Arc<Scheduler>,
    space: ExecSpace,
    trees: TreeOps,
    platform: BTreeMap<String, String>,
    requester: String,
}

impl PlanRunner {
    /// A runner for trees read through `trees`, submitting for `org` as
    /// `requester` with `platform` as every step's base properties.
    pub fn new(
        scheduler: Arc<Scheduler>,
        org: &str,
        trees: TreeOps,
        platform: BTreeMap<String, String>,
        requester: impl Into<String>,
    ) -> Self {
        PlanRunner {
            space: scheduler.space(org),
            scheduler,
            trees,
            platform,
            requester: requester.into(),
        }
    }

    /// Verify `target` by its own plan. A tree without a plan is green;
    /// the first failing step ends verification.
    pub async fn verify(&self, target: &Target) -> Result<VerificationOutcome> {
        let Some(plan) = load_plan(&self.trees, target.root).await? else {
            return Ok(VerificationOutcome {
                green: true,
                ..Default::default()
            });
        };
        let changed = match target.base {
            Some(base) => Some(
                diff_trees(&self.trees, Some(base), Some(target.root))
                    .await?
                    .into_iter()
                    .map(|entry| entry.path)
                    .collect::<Vec<_>>(),
            ),
            None => None,
        };
        let mut steps = Vec::new();
        let mut compiling = FuturesUnordered::new();
        for step in &plan.steps {
            match select(step, changed.as_deref(), target.risk) {
                Selection::Run => compiling.push(compile(
                    &self.space,
                    &self.trees,
                    target.root,
                    step,
                    &self.platform,
                )),
                Selection::Unaffected => {
                    steps.push(StepResult::skipped(&step.name, StepStatus::Unaffected))
                }
                Selection::NotGating => {
                    steps.push(StepResult::skipped(&step.name, StepStatus::NotGating))
                }
            }
        }
        let mut running = FuturesUnordered::new();
        while let Some(actions) = compiling.next().await {
            for action in actions? {
                running.push(self.run(action, target.priority));
            }
        }
        let mut green = true;
        while let Some(result) = running.next().await {
            let result = result?;
            let ok = result.status.is_ok();
            steps.push(result);
            if !ok {
                green = false;
                break;
            }
        }
        let mut outcome = VerificationOutcome {
            green,
            steps,
            excerpt: None,
        };
        if let Some(failed) = outcome.first_failure() {
            outcome.excerpt = self.excerpt(failed).await?;
        }
        Ok(outcome)
    }

    /// Run one action, or take its cached result.
    async fn run(&self, compiled: CompiledAction, priority: Priority) -> Result<StepResult> {
        let mut result = StepResult {
            action: Some(compiled.action),
            shard: compiled.shard,
            ..StepResult::skipped(&compiled.step, StepStatus::Passed)
        };
        if let Some(cached) = self.space.result(&compiled.action).await? {
            fill(&mut result, &cached);
            result.cached = true;
            return Ok(result);
        }
        let submitted = self
            .scheduler
            .submit(
                &self.space,
                Submission {
                    action: compiled.action,
                    platform: Some(compiled.platform),
                    priority,
                    requester: self.requester.clone(),
                    do_not_cache: false,
                },
            )
            .await?;
        let response = self.wait(&submitted.operation.id).await?;
        result.cached = response.cached_result;
        if let Some(action_result) = &response.result {
            fill(&mut result, action_result);
        }
        match response.status.filter(|s| s.code != 0) {
            Some(status) if status.code == tonic::Code::DeadlineExceeded as i32 => {
                result.status = StepStatus::TimedOut;
            }
            Some(status) => result.status = StepStatus::Error(status.message),
            None if response.result.is_none() => {
                result.status = StepStatus::Error("the worker returned no result".into());
            }
            None => {}
        }
        Ok(result)
    }

    /// The response of operation `id` once it completes.
    async fn wait(&self, id: &str) -> Result<ExecuteResponse> {
        loop {
            let seen = self.scheduler.operation_updates(&self.space, id).await?;
            let op = self
                .scheduler
                .operation(&self.space, id)
                .await?
                .ok_or_else(|| Error::InvalidAction(format!("operation {id} vanished")))?;
            if op.stage == Stage::Completed {
                let bytes = op.response.unwrap_or_default();
                return ExecuteResponse::decode(bytes)
                    .map_err(|e| Error::Codec(format!("operation {id}: {e}")));
            }
            self.scheduler
                .wait_operation(&self.space, id, seen, WAIT)
                .await?;
        }
    }

    /// The last bytes of a failed step's output, standard error first.
    async fn excerpt(&self, failed: &StepResult) -> Result<Option<String>> {
        for digest in [failed.stderr, failed.stdout].into_iter().flatten() {
            if digest.is_empty() {
                continue;
            }
            let from = digest.size.saturating_sub(EXCERPT_BYTES);
            if let Some(bytes) = self.space.read_range(&digest, from, EXCERPT_BYTES).await? {
                return Ok(Some(String::from_utf8_lossy(&bytes).into_owned()));
            }
        }
        Ok(None)
    }
}

/// Copy what an action produced into `result`.
fn fill(result: &mut StepResult, action: &rill_reapi_proto::reapi::ActionResult) {
    result.status = match action.exit_code {
        0 => StepStatus::Passed,
        exit_code => StepStatus::Failed { exit_code },
    };
    result.stdout = action
        .stdout_digest
        .as_ref()
        .and_then(|d| BlobDigest::parse(d).ok());
    result.stderr = action
        .stderr_digest
        .as_ref()
        .and_then(|d| BlobDigest::parse(d).ok());
    if let Some(meta) = &action.execution_metadata {
        result.worker = Some(meta.worker.clone()).filter(|w| !w.is_empty());
        result.duration = match (
            &meta.worker_start_timestamp,
            &meta.worker_completed_timestamp,
        ) {
            (Some(start), Some(end)) => {
                let millis = (end.seconds - start.seconds) * 1000
                    + i64::from(end.nanos - start.nanos) / 1_000_000;
                u64::try_from(millis).ok().map(Duration::from_millis)
            }
            _ => None,
        };
    }
}
