//! `rill_sync`: move a change onto a newer base (trunk by default).
//! Conflicts are recorded in the new version, not raised.

use rill_sdk::Root;
use serde_json::{json, Value};

use super::args::Args;
use super::hints::next_step;
use super::schema::{root, Props};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_sync",
        description: "Rebase a change onto `onto` (default the trunk head). Conflicts do not fail the call; they are listed, to settle with rill_conflicts and rill_resolve.",
        input_schema: Props::repo()
            .change()
            .opt("onto", root("New base (default trunk)"))
            .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let id = ctx.change(args)?;
    let current = repo.current_version(id).await?;
    let onto_root = args.opt_parse::<Root>("onto")?.unwrap_or_else(Root::trunk);
    let onto = repo.resolve_root(&onto_root).await?;
    if current.parents.first() == Some(&onto) {
        return Ok(json!({
            "change": id.to_string(),
            "version": current.hash.to_hex(),
            "onto": onto_root.to_string(),
            "moved": false,
            "conflicts": [],
            "next": next_step(current.has_conflicts as usize, None, None),
        }));
    }
    let result = repo.rebase(current.hash, onto).await?;
    let conflicts = result.conflicted_paths();
    Ok(json!({
        "change": id.to_string(),
        "version": result.hash.to_hex(),
        "onto": onto_root.to_string(),
        "moved": true,
        "conflicts": conflicts.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        "next": next_step(conflicts.len(), None, None),
    }))
}
