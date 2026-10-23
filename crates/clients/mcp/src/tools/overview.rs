//! `rill_context`: one call that orients an agent: who it is, the
//! repository and trunk, the change it is working on, and what to do next.

use serde_json::{json, Value};

use super::args::Args;
use super::hints::next_step;
use super::schema::Props;
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;
use crate::json;

const WORKFLOW: &str = "rill_read / rill_search to explore; rill_edit to change files (starts a change, then amends it); rill_check for conflicts and checks; rill_commit to describe and submit; rill_review_status and rill_watch to follow review; rill_respond to answer threads and vote; rill_land to land; rill_sync, rill_conflicts and rill_resolve when trunk moves under you.";

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_context",
        description: "Start here. Who you are, the repository and trunk head, the change this session is working on (or `change`) with its review status, the next step, and the workflow.",
        input_schema: Props::repo().change().build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let whoami = match ctx.client().admin().whoami().await {
        Ok(w) => json!({ "principal": json::principal(&w.principal), "teams": w.teams }),
        Err(e) => {
            tracing::debug!(error = %e, "whoami failed");
            Value::Null
        }
    };
    let info = repo.info().await?;
    let head = repo.trunk_head().await?;
    let landing = repo.trunk().queue().await?.len();
    let mut out = json!({
        "whoami": whoami,
        "repo": json::repo_info(&info),
        "trunk": {
            "head_seq": head.map_or(0, |(seq, _)| seq.0),
            "head": head.map(|(_, hash)| hash.to_hex()),
            "landing": landing,
        },
        "change": Value::Null,
        "workflow": WORKFLOW,
    });
    let Some(id) = ctx.opt_change(args)? else {
        out["next"] = json!("No current change: rill_edit starts one.");
        return Ok(out);
    };
    let version = repo.current_version(id).await?;
    let review = match repo.reviews().get(id).await {
        Ok(r) => Some(r.review),
        Err(e) if e.is_not_found() => None,
        Err(e) => return Err(e.into()),
    };
    out["change"] = json!({
        "id": id.to_string(),
        "version": version.hash.to_hex(),
        "summary": version.summary(),
        "files_changed": version.files_changed,
        "has_conflicts": version.has_conflicts,
        "status": review.as_ref().map_or(json!("draft"), |r| json::enum_name(&r.status)),
    });
    let decision = match &review {
        Some(_) => Some(repo.reviews().evaluate_submit(id).await?),
        None => None,
    };
    out["next"] = json!(next_step(
        version.has_conflicts as usize,
        review.as_ref(),
        decision.as_ref()
    ));
    Ok(out)
}
