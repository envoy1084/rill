//! `rill_resolve`: settle conflicted paths with merged content (or by
//! deleting them), producing a new version of the change.

use bytes::Bytes;
use rill_sdk::Resolution;
use serde_json::{json, Value};

use super::args::Args;
use super::hints::next_step;
use super::provenance::{file_kind, provenance};
use super::schema::{boolean, provenance_props, string, string_enum, Props};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_resolve",
        description: "Resolve conflicted paths of a change with their merged content, or by deleting them. Returns the new version and any paths still in conflict.",
        input_schema: provenance_props(Props::repo()
            .change()
            .req("resolutions", json!({
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "path": string("Conflicted path"),
                        "content": string("Merged content"),
                        "kind": string_enum("File kind (default regular)", &["regular", "executable", "symlink"]),
                        "delete": boolean("Resolve by deleting the path"),
                    },
                    "required": ["path"],
                    "additionalProperties": false,
                }
            }))
            )
            .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let id = ctx.change(args)?;
    let hash = repo.current_version(id).await?.hash;
    let resolutions = args
        .objects("resolutions")?
        .iter()
        .map(|r| {
            let path = r.path("path")?;
            Ok(match r.bool_or("delete", false)? {
                true => Resolution::Delete { path },
                false => Resolution::Set {
                    path,
                    bytes: Bytes::from(r.str("content")?.to_string()),
                    kind: file_kind(r)?,
                },
            })
        })
        .collect::<ToolResult<Vec<_>>>()?;
    let result = repo.resolve(hash, resolutions, provenance(args)?).await?;
    let remaining = result.conflicted_paths();
    Ok(json!({
        "change": id.to_string(),
        "version": result.hash.to_hex(),
        "conflicts": remaining.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        "next": next_step(remaining.len(), None, None),
    }))
}
