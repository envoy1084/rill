//! `rill_search`: regex search over a root, served from the server's
//! index and fitted to the response budget.

use rill_sdk::GrepQuery;
use serde_json::{json, Value};

use super::args::Args;
use super::budget::{self, Budget, Cursor};
use super::root::read_root;
use super::schema::{boolean, integer, root, string, Props};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;
use crate::json;

/// Hits fetched per search; the budget decides how many are returned.
const MAX_HITS: u32 = 1_000;

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_search",
        description: "Search file contents with a regular expression (Rust syntax). Searches the session's change by default, else trunk. Returns matching lines with optional context; cut to `max_tokens` with a next_cursor.",
        input_schema: budget::props(
            Props::repo()
                .req("pattern", string("Regular expression"))
                .opt("path", string("Only under this directory"))
                .opt("glob", string("Only files matching this glob, e.g. `*.rs`"))
                .opt("ignore_case", boolean("Case-insensitive matching"))
                .opt("context", integer("Lines of context around each match (default 0)"))
                .opt("at", root("Root to search"))
                .change(),
        )
        .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let root = read_root(ctx, &repo, args).await?;
    let mut query = GrepQuery::new(args.str("pattern")?)
        .case_insensitive(args.bool_or("ignore_case", false)?)
        .context_lines(args.limit("context", 0)?.min(20))
        .max_results(MAX_HITS);
    if let Some(glob) = args.opt_string("glob")? {
        query = query.glob(glob);
    }
    let dir = args.path_or_root("path")?;
    if !dir.is_root() {
        query = query.glob(format!("{dir}/**"));
    }
    let found = repo.grep(&root, &query).await?;
    let hits: Vec<Value> = found.hits.iter().map(json::grep_hit).collect();
    let mut budget = Budget::from_args(args)?;
    let (shown, next) = budget::take(&hits, Cursor::from_args(args)?.item, &mut budget);
    let mut out = json!({
        "root": root.to_string(),
        "total": hits.len(),
        "matches": shown,
    });
    budget::finish(&mut out, next);
    if found.truncated {
        out["more_on_server"] = json!(true);
    }
    Ok(out)
}
