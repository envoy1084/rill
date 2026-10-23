//! `rill_respond`: act on a review in one call: comment, reply, resolve or
//! reopen threads, and vote.

use rill_sdk::{NewComment, Reply, RepoClient};
use rill_types::objects::review::CommentCategory;
use rill_types::{ChangeId, RepoPathBuf, ThreadAnchor};
use serde_json::{json, Value};

use super::args::Args;
use super::schema::{integer, string, string_enum, Props};
use super::{ToolContext, ToolDef};
use crate::error::{ToolError, ToolResult};
use crate::json;

const CATEGORIES: &[&str] = &[
    "discussion",
    "bug",
    "security",
    "style",
    "test_gap",
    "question",
    "suggestion",
];

fn actions_schema() -> Value {
    json!({
        "type": "array",
        "description": "Actions, run in order",
        "items": {
            "type": "object",
            "properties": {
                "action": string_enum("What to do", &["comment", "reply", "resolve", "reopen", "vote"]),
                "body": string("comment/reply: the text"),
                "thread": string("reply/resolve/reopen: thread id"),
                "path": string("comment: anchor to this file"),
                "line": integer("comment: first line of the anchored range"),
                "end_line": integer("comment: last line (default `line`)"),
                "category": string_enum("comment/reply: category", CATEGORIES),
                "confidence": integer("comment/reply: confidence 0-100"),
                "label": string("vote: label, e.g. `Review` or `Verified`"),
                "value": integer("vote: value within the label's range"),
                "reason": string("vote: why"),
            },
            "required": ["action"],
            "additionalProperties": false,
        }
    })
}

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_respond",
        description: "Respond to a review in one call: open comment threads (on the change, a file or lines), reply to threads, resolve or reopen them, and vote. Actions run in order; returns each result and the open-thread count.",
        input_schema: Props::repo()
            .change()
            .req("actions", actions_schema())
            .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let id = ctx.change(args)?;
    let mut results = Vec::new();
    for action in args.objects("actions")? {
        results.push(act(&repo, id, &action).await?);
    }
    let review = repo.reviews().get(id).await?.review;
    Ok(json!({
        "change": id.to_string(),
        "results": results,
        "status": json::enum_name(&review.status),
        "open_threads": review.unresolved_threads(),
    }))
}

async fn act(repo: &RepoClient, id: ChangeId, a: &Args) -> ToolResult<Value> {
    let reviews = repo.reviews();
    Ok(match a.str("action")? {
        "comment" => {
            let new = NewComment {
                anchor: anchor(repo, id, a).await?,
                body: a.str("body")?.to_string(),
                category: a.opt_str("category").map(category).transpose()?,
                confidence: confidence(a)?,
                to: None,
            };
            let t = reviews.comment(id, new).await?;
            json!({ "action": "comment", "thread": t.thread_id })
        }
        "reply" => {
            let reply = Reply {
                body: a.str("body")?.to_string(),
                category: a.opt_str("category").map(category).transpose()?,
                confidence: confidence(a)?,
            };
            let t = reviews.reply(id, a.str("thread")?, reply).await?;
            json!({ "action": "reply", "thread": t.id })
        }
        verb @ ("resolve" | "reopen") => {
            let t = reviews
                .resolve_thread(id, a.str("thread")?, verb == "resolve")
                .await?;
            json!({ "action": verb, "thread": t.id, "resolved": t.resolved })
        }
        "vote" => {
            let value = a.i64("value")?;
            let value = i8::try_from(value)
                .map_err(|_| ToolError::invalid(format!("vote value {value} is out of range")))?;
            let label = a.str("label")?;
            reviews
                .vote(id, label, value, a.opt_string("reason")?, None)
                .await?;
            json!({ "action": "vote", "label": label, "value": value })
        }
        other => return Err(ToolError::invalid(format!("unknown action {other:?}"))),
    })
}

fn confidence(a: &Args) -> ToolResult<Option<u8>> {
    Ok(a.opt_u64("confidence")?.map(|c| c.min(100) as u8))
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

/// The change, a file, or a line range of the current version.
async fn anchor(repo: &RepoClient, id: ChangeId, a: &Args) -> ToolResult<ThreadAnchor> {
    let Some(path) = a.opt_str("path") else {
        return Ok(ThreadAnchor::Change);
    };
    let path = RepoPathBuf::new(path)?;
    let version = repo.current_version(id).await?.hash;
    let line = |key: &str| -> ToolResult<Option<u32>> {
        a.opt_u64(key)?
            .map(|n| u32::try_from(n).map_err(|_| ToolError::invalid(format!("`{key}` too large"))))
            .transpose()
    };
    Ok(match line("line")? {
        None => ThreadAnchor::File { version, path },
        Some(start) => ThreadAnchor::Lines {
            version,
            path,
            start,
            end: line("end_line")?.unwrap_or(start).max(start),
            context_hash: String::new(),
        },
    })
}
