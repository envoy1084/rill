//! `rill_watch`: wait for something to happen to a change's review
//! (votes, comments, new versions, landing) instead of polling.

use std::time::Duration;

use futures::StreamExt;
use serde_json::{json, Value};

use super::args::Args;
use super::schema::{integer, Props};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;
use crate::json;

const DEFAULT_TIMEOUT_SECS: u64 = 30;
const MAX_TIMEOUT_SECS: u64 = 300;
/// After the first event, how long to keep collecting related ones.
const SETTLE: Duration = Duration::from_millis(500);

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_watch",
        description: "Wait for review events on a change (status changes, votes, thread updates, new versions, land progress). Returns once something happens (gathering events that arrive together) or after `timeout_seconds` with no events.",
        input_schema: Props::repo()
            .change()
            .opt("timeout_seconds", integer("Longest wait (default 30, max 300)"))
            .opt("max_events", integer("Most events to return (default 20)"))
            .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let id = ctx.change(args)?;
    let timeout = Duration::from_secs(
        args.opt_u64("timeout_seconds")?
            .unwrap_or(DEFAULT_TIMEOUT_SECS)
            .min(MAX_TIMEOUT_SECS),
    );
    let max = args.limit("max_events", 20)?.max(1) as usize;
    let mut stream = repo.reviews().watch(id).await?;
    let mut events = Vec::new();
    let mut wait = timeout;
    while events.len() < max {
        match tokio::time::timeout(wait, stream.next()).await {
            Ok(Some(event)) => {
                events.push(json::review_event(&event?));
                wait = SETTLE;
            }
            Ok(None) | Err(_) => break,
        }
    }
    Ok(json!({
        "change": id.to_string(),
        "events": events,
        "timed_out": events.is_empty(),
        "next": if events.is_empty() {
            "Nothing happened; call rill_watch again or check rill_review_status."
        } else {
            "Read the events; rill_review_status has the full state."
        },
    }))
}
