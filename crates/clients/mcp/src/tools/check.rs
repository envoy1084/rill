//! `rill_check`: is the change healthy? Conflicts, and the verification
//! runs of its current version with the failing steps' log tails.

use std::time::{Duration, Instant};

use rill_proto::rill::v1 as pb;
use rill_sdk::ChecksClient;
use serde_json::{json, Value};

use super::args::Args;
use super::schema::{integer, Props};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;
use crate::json;

/// Bytes of a failing step's log returned, from its end.
const LOG_TAIL: u32 = 2_000;
const POLL: Duration = Duration::from_secs(2);
const MAX_WAIT_SECS: u64 = 600;

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_check",
        description: "Check a change: conflicted paths and verification runs of its current version (green, red or running), with the tail of each failing step's log. `wait_seconds` waits for running checks to finish.",
        input_schema: Props::repo()
            .change()
            .opt("wait_seconds", integer("Wait up to this long for running checks (default 0, max 600)"))
            .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let id = ctx.change(args)?;
    let info = repo.current_version(id).await?;
    let version = repo.change_version(info.hash).await?;
    let conflicts: Vec<&str> = version.conflicted_paths().map(|p| p.as_str()).collect();
    let checks = repo.checks();
    let wait = Duration::from_secs(
        args.opt_u64("wait_seconds")?
            .unwrap_or(0)
            .min(MAX_WAIT_SECS),
    );
    let deadline = Instant::now() + wait;
    let runs = loop {
        let runs = checks.runs(Some(&info.hash), 20, None).await?.items;
        if runs.iter().all(|r| r.green.is_some()) || Instant::now() + POLL > deadline {
            break runs;
        }
        tokio::time::sleep(POLL).await;
    };
    let mut shown = Vec::with_capacity(runs.len());
    for run in &runs {
        shown.push(run_summary(&checks, run).await?);
    }
    let verdict = if runs.is_empty() {
        "none"
    } else if runs.iter().any(|r| r.green == Some(false)) {
        "red"
    } else if runs.iter().any(|r| r.green.is_none()) {
        "running"
    } else {
        "green"
    };
    let next = match (conflicts.len(), verdict) {
        (n, _) if n > 0 => format!("{n} path(s) conflict: rill_conflicts, then rill_resolve."),
        (_, "red") => "Checks failed: read the failing steps, fix with rill_edit, then rill_check again.".into(),
        (_, "running") => "Checks are running: call again with `wait_seconds`.".into(),
        (_, "none") => "No checks have run on this version; they run when it lands. Submit it with rill_commit.".into(),
        _ => "Checks are green: rill_commit to submit, or rill_land if approved.".into(),
    };
    Ok(json!({
        "change": id.to_string(),
        "version": info.hash.to_hex(),
        "conflicts": conflicts,
        "verdict": verdict,
        "runs": shown,
        "next": next,
    }))
}

/// A run with only its failing steps, each with its log tail.
async fn run_summary(checks: &ChecksClient, run: &pb::VerifyRun) -> ToolResult<Value> {
    let mut out = json!({
        "id": run.id,
        "status": json::run_status(run),
        "summary": run.summary,
    });
    if run.green != Some(false) {
        return Ok(out);
    }
    let full = checks.run(&run.id).await?;
    let mut failed = Vec::new();
    for step in full.steps.iter().filter(|s| json::step_failed(s)) {
        let mut s = json::run_step(step);
        let stderr = step.stderr_size > 0;
        let log = checks
            .read_log(&run.id, step.index, stderr, -(LOG_TAIL as i64), LOG_TAIL)
            .await?;
        s["log_tail"] = json!(String::from_utf8_lossy(&log.data));
        failed.push(s);
    }
    out["failed_steps"] = json!(failed);
    Ok(out)
}
