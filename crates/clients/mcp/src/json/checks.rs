//! Verification runs and their steps.

use rill_proto::rill::v1 as pb;
use serde_json::{json, Value};

/// A step's state as shown to people and agents.
pub fn step_state(value: i32) -> &'static str {
    match rill_proto::convert::known::<pb::StepState>(value).unwrap_or(pb::StepState::Unspecified) {
        pb::StepState::Queued => "queued",
        pb::StepState::Running => "running",
        pb::StepState::Passed => "passed",
        pb::StepState::Failed => "failed",
        pb::StepState::TimedOut => "timed out",
        pb::StepState::Error => "error",
        pb::StepState::Unaffected => "unaffected",
        pb::StepState::NotGating => "not gating",
        pb::StepState::Unspecified => "-",
    }
}

/// True for a step that ran and did not pass.
pub fn step_failed(step: &pb::RunStep) -> bool {
    matches!(
        rill_proto::convert::known::<pb::StepState>(step.state.to_i32()),
        Ok(pb::StepState::Failed | pb::StepState::TimedOut | pb::StepState::Error)
    )
}

/// `running` until the run ends, then `green` or `red`.
pub fn run_status(run: &pb::VerifyRun) -> &'static str {
    match run.green {
        None => "running",
        Some(true) => "green",
        Some(false) => "red",
    }
}

/// One step of a run.
pub fn run_step(s: &pb::RunStep) -> Value {
    json!({
        "index": s.index,
        "name": s.name,
        "shard": (s.shards > 0).then_some((s.shard, s.shards)),
        "state": step_state(s.state.to_i32()),
        "exit_code": s.exit_code,
        "error": s.error,
        "cached": s.cached,
        "stdout_size": s.stdout_size,
        "stderr_size": s.stderr_size,
        "artifacts": s.artifacts.iter().map(|a| json!({"path": a.path, "size": a.size})).collect::<Vec<_>>(),
        "worker": s.worker,
        "duration_ms": s.duration_ms,
    })
}

/// A run with its steps.
pub fn verify_run(run: &pb::VerifyRun) -> Value {
    json!({
        "id": run.id,
        "version": hex::encode(&run.version),
        "trunk": run.trunk,
        "status": run_status(run),
        "summary": run.summary,
        "started_at": run.started_at,
        "finished_at": run.finished_at,
        "steps": run.steps.iter().map(run_step).collect::<Vec<_>>(),
    })
}
