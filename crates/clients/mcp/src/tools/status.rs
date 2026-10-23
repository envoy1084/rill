//! `rill_review_status`: where a change stands in review: status, votes
//! on the current version, open threads, what policy still needs, and
//! the next step.

use serde_json::{json, Value};

use super::args::Args;
use super::budget::{self, Budget, Cursor};
use super::hints::next_step;
use super::schema::{boolean, Props};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;
use crate::json;

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_review_status",
        description: "Review status of a change: status, votes on the current version, open threads (resolved ones with `all_threads`), whether policy allows landing and why not, and the next step.",
        input_schema: budget::props(
            Props::repo()
                .change()
                .opt("all_threads", boolean("Include resolved threads")),
        )
        .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let id = ctx.change(args)?;
    let reviews = repo.reviews();
    let info = match reviews.get(id).await {
        Ok(info) => info,
        Err(e) if e.is_not_found() => {
            let version = repo.current_version(id).await?;
            return Ok(json!({
                "change": id.to_string(),
                "status": "draft",
                "next": next_step(version.has_conflicts as usize, None, None),
            }));
        }
        Err(e) => return Err(e.into()),
    };
    let review = &info.review;
    let decision = reviews.evaluate_submit(id).await?;
    let all = args.bool_or("all_threads", false)?;
    let threads: Vec<Value> = review
        .threads
        .iter()
        .filter(|t| all || !t.resolved)
        .map(json::thread)
        .collect();
    let mut budget = Budget::from_args(args)?;
    let (shown, next) = budget::take(&threads, Cursor::from_args(args)?.item, &mut budget);
    let conflicts = match review.current_version() {
        Some(hash) => repo.change_info(hash).await?.has_conflicts as usize,
        None => 0,
    };
    let mut out = json!({
        "change": id.to_string(),
        "status": json::enum_name(&review.status),
        "version": review.current_version().map(|h| h.to_hex()),
        "risk": json::enum_name(&review.risk),
        "votes": review.current_votes().map(|v| json!({
            "label": v.label,
            "value": v.value,
            "voter": json::principal(&v.voter),
            "reason": v.reason,
        })).collect::<Vec<_>>(),
        "open_threads": review.unresolved_threads(),
        "threads": shown,
        "decision": json::submit_decision(&decision),
        "landed_seq": review.landed_seq.map(|s| s.0),
        "next": next_step(conflicts, Some(review), Some(&decision)),
    });
    budget::finish(&mut out, next);
    Ok(out)
}
