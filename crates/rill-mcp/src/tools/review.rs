//! Review tools: submit, vote, comment, evaluate and landing.

use rill_sdk::NewComment;
use rill_types::objects::review::CommentCategory;
use rill_types::{ChangeId, ChangeVersionHash, RepoPathBuf, ThreadAnchor};
use serde_json::Value;

use super::args::Args;
use super::schema::{integer, string, string_enum, Props};
use super::{ToolContext, ToolDef};
use crate::error::{ToolError, ToolResult};
use crate::json;

/// Tool definitions in this group.
pub fn definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "rill_submit",
            description: "Submit a change for review, moving it from draft to open.",
            input_schema: Props::repo().req("change_id", string("Change id")).build(),
        },
        ToolDef {
            name: "rill_vote",
            description: "Cast a label vote (for example Review +2 or Verified -1) on a change's current version.",
            input_schema: Props::repo()
                .req("change_id", string("Change id"))
                .req("label", string("Label name, e.g. `Review`, `Verified`, `Owner`"))
                .req("value", integer("Vote value within the label's range"))
                .opt("reason", string("Why you voted this way"))
                .opt("version", string("Version hash to vote on (default current)"))
                .build(),
        },
        ToolDef {
            name: "rill_comment",
            description: "Open a review thread on a change, a file, or a line range.",
            input_schema: Props::repo()
                .req("change_id", string("Change id"))
                .req("body", string("Comment text"))
                .opt("path", string("Anchor the thread to this file"))
                .opt("line", integer("First line of the anchored range (requires path)"))
                .opt("end_line", integer("Last line of the range (default same as line)"))
                .opt("version", string("Version hash the anchor refers to (default current)"))
                .opt("category", string_enum("Comment category", &["discussion", "bug", "security", "style", "test_gap", "question", "suggestion"]))
                .opt("confidence", integer("Reviewer confidence, 0-100"))
                .build(),
        },
        ToolDef {
            name: "rill_evaluate",
            description: "Evaluate the submit policy for a change: whether it may land and why.",
            input_schema: Props::repo().req("change_id", string("Change id")).build(),
        },
        ToolDef {
            name: "rill_request_land",
            description: "Ask the lander to land a change on trunk. Returns a ticket to poll with rill_land_status.",
            input_schema: Props::repo().req("change_id", string("Change id")).build(),
        },
        ToolDef {
            name: "rill_land_status",
            description: "Status of a land ticket: queued, rebasing, landed (with sequence), failed or cancelled.",
            input_schema: Props::repo().req("ticket", string("Land ticket id")).build(),
        },
    ]
}

/// Dispatch a call in this group.
pub async fn call(ctx: &ToolContext, name: &str, args: &Args) -> Option<ToolResult<Value>> {
    Some(match name {
        "rill_submit" => submit(ctx, args).await,
        "rill_vote" => vote(ctx, args).await,
        "rill_comment" => comment(ctx, args).await,
        "rill_evaluate" => evaluate(ctx, args).await,
        "rill_request_land" => request_land(ctx, args).await,
        "rill_land_status" => land_status(ctx, args).await,
        _ => return None,
    })
}

async fn submit(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let reviews = ctx.repo(args)?.reviews();
    let state = reviews.submit(args.parse::<ChangeId>("change_id")?).await?;
    Ok(json::review_state(&state))
}

async fn vote(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let reviews = ctx.repo(args)?.reviews();
    let value = args.i64("value")?;
    let value = i8::try_from(value)
        .map_err(|_| ToolError::invalid(format!("vote value {value} is out of range")))?;
    let state = reviews
        .vote(
            args.parse::<ChangeId>("change_id")?,
            args.str("label")?,
            value,
            args.opt_string("reason")?,
            args.opt_parse::<ChangeVersionHash>("version")?,
        )
        .await?;
    Ok(json::review_state(&state))
}

fn category(name: &str) -> ToolResult<CommentCategory> {
    Ok(match name {
        "discussion" => CommentCategory::Discussion,
        "bug" => CommentCategory::Bug,
        "security" => CommentCategory::Security,
        "style" => CommentCategory::Style,
        "test_gap" => CommentCategory::TestGap,
        "question" => CommentCategory::Question,
        "suggestion" => CommentCategory::Suggestion,
        other => return Err(ToolError::invalid(format!("unknown category {other:?}"))),
    })
}

/// Build the anchor for a comment from `path`, `line`, `end_line` and
/// `version`, resolving the version when a path is anchored.
pub async fn anchor(
    repo: &rill_sdk::RepoClient,
    change_id: ChangeId,
    args: &Args,
) -> ToolResult<ThreadAnchor> {
    let Some(path) = args.opt_str("path") else {
        return Ok(ThreadAnchor::Change);
    };
    let path = RepoPathBuf::new(path)?;
    let version = match args.opt_parse::<ChangeVersionHash>("version")? {
        Some(v) => v,
        None => repo.current_version(change_id).await?.hash,
    };
    Ok(match args.opt_u64("line")? {
        None => ThreadAnchor::File { version, path },
        Some(start) => {
            let start = u32::try_from(start).map_err(|_| ToolError::invalid("`line` too large"))?;
            let end = args
                .opt_u64("end_line")?
                .map(|e| u32::try_from(e).map_err(|_| ToolError::invalid("`end_line` too large")))
                .transpose()?
                .unwrap_or(start);
            ThreadAnchor::Lines {
                version,
                path,
                start,
                end: end.max(start),
                context_hash: String::new(),
            }
        }
    })
}

async fn comment(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let change_id = args.parse::<ChangeId>("change_id")?;
    let confidence = args
        .opt_u64("confidence")?
        .map(|c| u8::try_from(c.min(100)).unwrap_or(100));
    let new = NewComment {
        anchor: anchor(&repo, change_id, args).await?,
        body: args.str("body")?.to_string(),
        category: args.opt_str("category").map(category).transpose()?,
        confidence,
        to: None,
    };
    let thread = repo.reviews().comment(change_id, new).await?;
    Ok(json::new_thread(&thread))
}

async fn evaluate(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let reviews = ctx.repo(args)?.reviews();
    let decision = reviews
        .evaluate_submit(args.parse::<ChangeId>("change_id")?)
        .await?;
    Ok(json::submit_decision(&decision))
}

async fn request_land(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let reviews = ctx.repo(args)?.reviews();
    let ticket = reviews
        .request_land(args.parse::<ChangeId>("change_id")?)
        .await?;
    Ok(json::land_ticket(&ticket))
}

async fn land_status(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let reviews = ctx.repo(args)?.reviews();
    let progress = reviews.land_status(args.str("ticket")?).await?;
    Ok(json::land_progress(&progress))
}
