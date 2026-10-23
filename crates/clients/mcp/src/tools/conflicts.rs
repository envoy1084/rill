//! `rill_conflicts`: each conflicted path of a change as structured
//! sides and bases, so an agent can merge them and call rill_resolve.

use rill_sdk::RepoClient;
use rill_types::objects::change::FileState;
use rill_types::FileChange;
use serde_json::{json, Value};

use super::args::Args;
use super::budget::{self, Budget, Cursor};
use super::schema::Props;
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;
use crate::json;

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_conflicts",
        description: "List a change's conflicted paths. Each has `sides` (the competing contents) and `bases` (what they started from); a null side means that side deleted the file. Merge them and pass the result to rill_resolve.",
        input_schema: budget::props(Props::repo().change()).build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let id = ctx.change(args)?;
    let hash = repo.current_version(id).await?.hash;
    let version = repo.change_version(hash).await?;
    let mut paths = Vec::new();
    for (path, change) in &version.file_changes {
        let FileChange::Conflict(merge) = change else {
            continue;
        };
        let mut sides = Vec::new();
        for state in merge.adds() {
            sides.push(state_json(&repo, state).await?);
        }
        let mut bases = Vec::new();
        for state in merge.removes() {
            bases.push(state_json(&repo, state).await?);
        }
        paths.push(json!({ "path": path.as_str(), "sides": sides, "bases": bases }));
    }
    let mut budget = Budget::from_args(args)?;
    let (shown, next) = budget::take(&paths, Cursor::from_args(args)?.item, &mut budget);
    let mut out = json!({
        "change": id.to_string(),
        "version": hash.to_hex(),
        "count": paths.len(),
        "conflicts": shown,
        "next": if paths.is_empty() {
            "No conflicts."
        } else {
            "Merge each path's sides and call rill_resolve with the results."
        },
    });
    budget::finish(&mut out, next);
    Ok(out)
}

async fn state_json(repo: &RepoClient, state: &FileState) -> ToolResult<Value> {
    let Some((content, kind)) = state else {
        return Ok(Value::Null);
    };
    let data = repo.get_blob(content).await?;
    let mut out = json::bytes(&data);
    out["kind"] = json::enum_name(kind);
    Ok(out)
}
