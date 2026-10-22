//! Intent tools: create, list, get, link and similarity search.

use rill_sdk::{IntentFilter, NewIntent};
use rill_types::{ChangeId, IntentId, IntentKind, IntentStatus};
use serde_json::{json, Value};

use super::args::Args;
use super::schema::{integer, string, string_enum, Props};
use super::{ToolContext, ToolDef};
use crate::error::{ToolError, ToolResult};
use crate::json;

const KINDS: &[&str] = &[
    "feature",
    "bug",
    "refactor",
    "chore",
    "experiment",
    "system",
];
const STATUSES: &[&str] = &["open", "in_progress", "done", "abandoned"];

/// Tool definitions in this group.
pub fn definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "rill_intent_create",
            description:
                "Create an intent (a task, bug, plan or spec) that changes can be linked to.",
            input_schema: Props::repo()
                .req("title", string("Short title"))
                .opt("body", string("Longer description"))
                .opt(
                    "kind",
                    string_enum("Kind of intent (default feature)", KINDS),
                )
                .opt("parent", string("Parent intent id"))
                .build(),
        },
        ToolDef {
            name: "rill_intent_list",
            description: "List intents, optionally filtered by status, kind, owner or parent.",
            input_schema: Props::repo()
                .opt(
                    "status",
                    json!({
                        "type": "array",
                        "description": "Only these statuses",
                        "items": { "type": "string", "enum": STATUSES },
                    }),
                )
                .opt(
                    "kind",
                    json!({
                        "type": "array",
                        "description": "Only these kinds",
                        "items": { "type": "string", "enum": KINDS },
                    }),
                )
                .opt(
                    "owner",
                    string("Only intents owned by this principal, as shown (ann, agent:bot)"),
                )
                .opt("parent", string("Only children of this intent"))
                .opt("limit", integer("Page size"))
                .opt("page_token", string("Continue from a previous page"))
                .build(),
        },
        ToolDef {
            name: "rill_intent_get",
            description: "Fetch one intent by id.",
            input_schema: Props::repo().req("id", string("Intent id")).build(),
        },
        ToolDef {
            name: "rill_intent_link",
            description: "Link a change to an intent.",
            input_schema: Props::repo()
                .req("intent", string("Intent id"))
                .req("change_id", string("Change id"))
                .build(),
        },
        ToolDef {
            name: "rill_intent_similar",
            description: "Find intents similar to a title and body, to avoid duplicating work.",
            input_schema: Props::repo()
                .req("title", string("Title text to match"))
                .opt("body", string("Body text to match"))
                .opt("limit", integer("Maximum hits (default 10)"))
                .build(),
        },
    ]
}

/// Dispatch a call in this group.
pub async fn call(ctx: &ToolContext, name: &str, args: &Args) -> Option<ToolResult<Value>> {
    Some(match name {
        "rill_intent_create" => create(ctx, args).await,
        "rill_intent_list" => list(ctx, args).await,
        "rill_intent_get" => get(ctx, args).await,
        "rill_intent_link" => link(ctx, args).await,
        "rill_intent_similar" => similar(ctx, args).await,
        _ => return None,
    })
}

/// Parse an intent kind name.
pub fn kind(name: &str) -> ToolResult<IntentKind> {
    Ok(match name {
        "feature" => IntentKind::Feature,
        "bug" => IntentKind::Bug,
        "refactor" => IntentKind::Refactor,
        "chore" => IntentKind::Chore,
        "experiment" => IntentKind::Experiment,
        "system" => IntentKind::System,
        other => return Err(ToolError::invalid(format!("unknown intent kind {other:?}"))),
    })
}

/// Parse an intent status name.
pub fn status(name: &str) -> ToolResult<IntentStatus> {
    Ok(match name {
        "open" => IntentStatus::Open,
        "in_progress" => IntentStatus::InProgress,
        "done" => IntentStatus::Done,
        "abandoned" => IntentStatus::Abandoned,
        other => {
            return Err(ToolError::invalid(format!(
                "unknown intent status {other:?}"
            )))
        }
    })
}

async fn create(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let intents = ctx.repo(args)?.intents();
    let mut new = NewIntent::new(
        args.str("title")?,
        kind(args.opt_str("kind").unwrap_or("feature"))?,
    );
    if let Some(body) = args.opt_string("body")? {
        new = new.with_body(body);
    }
    new.parent = args.opt_parse::<IntentId>("parent")?;
    Ok(json::intent(&intents.create(new).await?))
}

async fn list(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let intents = ctx.repo(args)?.intents();
    let filter = IntentFilter {
        status: args
            .opt_str_list("status")?
            .iter()
            .map(|s| status(s))
            .collect::<ToolResult<_>>()?,
        kind: args
            .opt_str_list("kind")?
            .iter()
            .map(|k| kind(k))
            .collect::<ToolResult<_>>()?,
        owner: args.opt_string("owner")?,
        parent: args.opt_parse::<IntentId>("parent")?,
        limit: args.limit("limit", 0)?,
        page_token: args.opt_string("page_token")?,
    };
    let page = intents.list(filter).await?;
    Ok(json::page(&page, json::intent))
}

async fn get(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let intents = ctx.repo(args)?.intents();
    Ok(json::intent(
        &intents.get(args.parse::<IntentId>("id")?).await?,
    ))
}

async fn link(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let intents = ctx.repo(args)?.intents();
    let intent = intents
        .link_change(
            args.parse::<IntentId>("intent")?,
            args.parse::<ChangeId>("change_id")?,
        )
        .await?;
    Ok(json::intent(&intent))
}

async fn similar(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let intents = ctx.repo(args)?.intents();
    let hits = intents
        .find_similar(
            args.str("title")?,
            args.opt_str("body").unwrap_or_default(),
            args.limit("limit", 10)?,
        )
        .await?;
    Ok(json!({ "hits": hits.iter().map(json::scored_intent).collect::<Vec<_>>() }))
}
