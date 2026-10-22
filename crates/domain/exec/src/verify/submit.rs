//! Running one compiled action through the scheduler, or taking its
//! cached result.

use std::time::Duration;

use prost::Message;
use rill_jobs::Priority;
use rill_reapi_proto::reapi::{ActionResult, ExecuteResponse};

use super::compile::CompiledAction;
use super::outcome::{StepResult, StepStatus};
use super::run::PlanRunner;
use crate::error::{Error, Result};
use crate::remote::{BlobDigest, Stage, Submission};

/// How long one wait for an operation lasts before it is read again.
const WAIT: Duration = Duration::from_secs(30);
/// Bytes of a failure's output kept in the outcome.
const EXCERPT_BYTES: u64 = 4096;

impl PlanRunner {
    /// Run `compiled`, or take its cached result; the step's result and,
    /// when the action produced one, its REAPI result.
    pub(super) async fn submit(
        &self,
        compiled: CompiledAction,
        priority: Priority,
    ) -> Result<(StepResult, Option<ActionResult>)> {
        let mut result = StepResult {
            action: Some(compiled.action),
            shard: compiled.shard,
            ..StepResult::skipped(&compiled.step, StepStatus::Passed)
        };
        if let Some(cached) = self.space.result(&compiled.action).await? {
            fill(&mut result, &cached);
            result.cached = true;
            return Ok((result, Some(cached)));
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
        Ok((result, response.result))
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
    pub(super) async fn excerpt(&self, failed: &StepResult) -> Result<Option<String>> {
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
fn fill(result: &mut StepResult, action: &ActionResult) {
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
