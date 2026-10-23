//! `rill_land`: ask the lander to land a change and wait for the outcome,
//! turning a failure into the step that fixes it.

use std::time::Duration;

use rill_sdk::{Error, LandProgress, LandStatus};
use serde_json::{json, Value};

use super::args::Args;
use super::schema::{integer, string, Props};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;

const DEFAULT_WAIT_SECS: u64 = 60;
const MAX_WAIT_SECS: u64 = 600;

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_land",
        description: "Land a change on trunk and wait for the outcome (`wait_seconds`, default 60; 0 returns at once). Returns `landed` with its trunk sequence, `pending` with a ticket to pass back to keep waiting, or `failed` with why and what to do.",
        input_schema: Props::repo()
            .change()
            .opt("ticket", string("Keep waiting on this land ticket instead of requesting a new landing"))
            .opt("wait_seconds", integer("How long to wait (default 60, max 600)"))
            .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let reviews = repo.reviews();
    let (ticket, change) = match args.opt_string("ticket")? {
        Some(ticket) => {
            let progress = reviews.land_status(&ticket).await?;
            (ticket, progress.ticket.change_id)
        }
        None => {
            let id = ctx.change(args)?;
            (reviews.request_land(id).await?.id, id)
        }
    };
    let wait = args
        .opt_u64("wait_seconds")?
        .unwrap_or(DEFAULT_WAIT_SECS)
        .min(MAX_WAIT_SECS);
    let progress = match repo
        .stack()
        .await_land(&ticket, Duration::from_secs(wait))
        .await
    {
        Ok(p) => p,
        Err(Error::LandFailed { .. } | Error::Timeout(_)) => reviews.land_status(&ticket).await?,
        Err(e) => return Err(e.into()),
    };
    if progress.status() == LandStatus::Landed {
        ctx.session().clear(change);
    }
    Ok(outcome(&progress))
}

fn outcome(p: &LandProgress) -> Value {
    let mut out = json!({
        "change": p.ticket.change_id.to_string(),
        "ticket": p.ticket.id,
    });
    match p.status() {
        LandStatus::Landed => {
            out["outcome"] = json!("landed");
            out["seq"] = json!(p.seq.map(|s| s.0));
            out["version"] = json!(p.landed_hash.map(|h| h.to_hex()));
            out["next"] = json!("Landed. Start the next change with rill_edit.");
        }
        LandStatus::Failed | LandStatus::Cancelled => {
            let error = p.error.clone().unwrap_or_default();
            out["outcome"] = json!("failed");
            out["error"] = json!(error);
            out["next"] = json!(failure_step(&error));
        }
        _ => {
            out["outcome"] = json!("pending");
            out["position"] = json!(p.position);
            out["eta_ms"] = json!(p.eta.map(|d| d.as_millis() as u64));
            out["next"] =
                json!("Still landing: call rill_land with this `ticket` to keep waiting.");
        }
    }
    out
}

/// The step that most likely fixes a landing that failed with `error`.
fn failure_step(error: &str) -> &'static str {
    let e = error.to_ascii_lowercase();
    if e.contains("conflict") {
        "Trunk moved and the change conflicts: rill_sync, then rill_conflicts and rill_resolve, then land again."
    } else if e.contains("check")
        || e.contains("verif")
        || e.contains("test")
        || e.contains("build")
    {
        "Checks failed on landing: rill_check shows the failing steps; fix with rill_edit and land again."
    } else if e.contains("polic")
        || e.contains("approv")
        || e.contains("vote")
        || e.contains("submit")
    {
        "Policy does not allow landing yet: rill_review_status shows what is missing."
    } else {
        "Read the error; rill_review_status and rill_check show the change's state."
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failures_point_at_the_step_that_fixes_them() {
        assert!(failure_step("rebase onto trunk@4 conflicts in a.rs").contains("rill_sync"));
        assert!(failure_step("verification failed: 2 steps red").contains("rill_check"));
        assert!(failure_step("submit policy not satisfied").contains("rill_review_status"));
    }
}
