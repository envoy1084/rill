//! History-writing tools: edits, rebase and conflict resolution.

use bytes::Bytes;
use rill_sdk::{ApplyOptions, Edit, EditBase, Resolution, Root};
use rill_types::{ChangeId, ChangeVersionHash, FileKind, IntentId, Provenance};
use serde_json::{json, Value};

use super::args::Args;
use super::schema::{provenance_props, root, string, string_enum, Props};
use super::{ToolContext, ToolDef};
use crate::error::{ToolError, ToolResult};
use crate::json;

/// Tool name recorded in every change's provenance.
pub const TOOL_NAME: &str = "rill-mcp";

fn edit_schema() -> Value {
    json!({
        "type": "array",
        "description": "Edits to apply, in order",
        "items": {
            "type": "object",
            "properties": {
                "op": string_enum("What to do with the path", &["set", "delete", "patch"]),
                "path": string("Repository path"),
                "content": string("New file content (op=set)"),
                "kind": string_enum("File kind for op=set (default regular)", &["regular", "executable", "symlink"]),
                "unified": string("Unified diff to apply to the base content (op=patch)"),
            },
            "required": ["op", "path"],
            "additionalProperties": false,
        }
    })
}

fn apply_edits_schema() -> Value {
    provenance_props(
        Props::repo()
            .req("message", string("Change message"))
            .req("edits", edit_schema())
            .opt("base", root("Point in history to edit on top of (default trunk); ignored when change_id is given without amend=false"))
            .opt("change_id", string("Amend this change's current version instead of creating a new change"))
            .opt("intent", string("Intent id the change belongs to")),
    )
    .build()
}

/// Tool definitions in this group.
pub fn definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "rill_apply_edits",
            description: "Create a change version from a list of edits (set, delete or patch a path) on top of a root, or amend an existing change. Provenance records this tool plus the model and session you pass.",
            input_schema: apply_edits_schema(),
        },
        ToolDef {
            name: "rill_create_change_from_edits",
            description: "Alias of rill_apply_edits that always creates a new change (never amends).",
            input_schema: provenance_props(
                Props::repo()
                    .req("message", string("Change message"))
                    .req("edits", edit_schema())
                    .opt("base", root("Point in history to build on (default trunk)"))
                    .opt("intent", string("Intent id the change belongs to")),
            )
            .build(),
        },
        ToolDef {
            name: "rill_rebase",
            description: "Rebase a change's current version (or a specific version) onto a new parent. Conflicts are recorded in the result, not raised.",
            input_schema: Props::repo()
                .opt("change_id", string("Change whose current version to rebase"))
                .opt("version", string("Version hash to rebase (alternative to change_id)"))
                .opt("onto", root("New parent (default trunk head)"))
                .build(),
        },
        ToolDef {
            name: "rill_resolve",
            description: "Resolve conflicted paths of a version by setting their content or deleting them, producing a new version.",
            input_schema: provenance_props(
                Props::repo()
                    .opt("change_id", string("Change whose current version to resolve"))
                    .opt("version", string("Version hash to resolve (alternative to change_id)"))
                    .req("resolutions", json!({
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "path": string("Conflicted path"),
                                "content": string("Resolved content; omit with delete=true"),
                                "kind": string_enum("File kind (default regular)", &["regular", "executable", "symlink"]),
                                "delete": json!({ "type": "boolean", "description": "Resolve by removing the path" }),
                            },
                            "required": ["path"],
                            "additionalProperties": false,
                        }
                    })),
            )
            .build(),
        },
    ]
}

/// Dispatch a call in this group.
pub async fn call(ctx: &ToolContext, name: &str, args: &Args) -> Option<ToolResult<Value>> {
    Some(match name {
        "rill_apply_edits" => apply_edits(ctx, args, true).await,
        "rill_create_change_from_edits" => apply_edits(ctx, args, false).await,
        "rill_rebase" => rebase(ctx, args).await,
        "rill_resolve" => resolve(ctx, args).await,
        _ => return None,
    })
}

/// Provenance for a write: this tool plus whatever the agent passed.
pub fn provenance(args: &Args) -> ToolResult<Provenance> {
    Ok(Provenance {
        tool: Some(TOOL_NAME.into()),
        model: args.opt_string("model")?,
        session: args.opt_string("session")?,
        prompt_ref: args.opt_string("prompt_ref")?,
        ..Provenance::default()
    })
}

fn file_kind(args: &Args) -> ToolResult<FileKind> {
    Ok(match args.opt_str("kind") {
        None | Some("regular") => FileKind::Regular,
        Some("executable") => FileKind::Executable,
        Some("symlink") => FileKind::Symlink,
        Some(other) => return Err(ToolError::invalid(format!("unknown file kind {other:?}"))),
    })
}

fn parse_edits(args: &Args) -> ToolResult<Vec<Edit>> {
    args.objects("edits")?
        .iter()
        .map(|e| {
            let path = e.path("path")?;
            Ok(match e.str("op")? {
                "set" => Edit::Set {
                    path,
                    bytes: Bytes::from(e.str("content")?.to_string()),
                    kind: file_kind(e)?,
                },
                "delete" => Edit::Delete { path },
                "patch" => Edit::Patch {
                    path,
                    unified: e.str("unified")?.to_string(),
                },
                other => return Err(ToolError::invalid(format!("unknown edit op {other:?}"))),
            })
        })
        .collect()
}

async fn apply_edits(ctx: &ToolContext, args: &Args, allow_amend: bool) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let edits = parse_edits(args)?;
    let mut opts = ApplyOptions::message(args.str("message")?).with_provenance(provenance(args)?);
    if let Some(intent) = args.opt_parse::<IntentId>("intent")? {
        opts = opts.with_intent(intent);
    }
    let amend = if allow_amend {
        args.opt_parse::<ChangeId>("change_id")?
    } else {
        None
    };
    // Amending edits on top of the change's own current version unless the
    // caller pins a base, in which case the amend moves the change there.
    let base = match (amend, args.opt_parse::<Root>("base")?) {
        (Some(id), None) => EditBase::Change(id),
        (Some(id), Some(root)) => {
            opts = opts.amending(id);
            EditBase::Root(root)
        }
        (None, root) => EditBase::Root(root.unwrap_or_else(Root::trunk)),
    };
    let result = repo.apply_edits(base, edits, opts).await?;
    Ok(json::version_result(&result))
}

async fn version_of(repo: &rill_sdk::RepoClient, args: &Args) -> ToolResult<ChangeVersionHash> {
    if let Some(hash) = args.opt_parse::<ChangeVersionHash>("version")? {
        return Ok(hash);
    }
    let id = args
        .opt_parse::<ChangeId>("change_id")?
        .ok_or_else(|| ToolError::invalid("`change_id` or `version` is required"))?;
    Ok(repo.current_version(id).await?.hash)
}

async fn rebase(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let version = version_of(&repo, args).await?;
    let onto = repo.resolve_root(&args.root_or_trunk("onto")?).await?;
    let result = repo.rebase(version, onto).await?;
    Ok(json::version_result(&result))
}

async fn resolve(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let version = version_of(&repo, args).await?;
    let resolutions = args
        .objects("resolutions")?
        .iter()
        .map(|r| {
            let path = r.path("path")?;
            Ok(if r.bool_or("delete", false)? {
                Resolution::Delete { path }
            } else {
                Resolution::Set {
                    path,
                    bytes: Bytes::from(r.str("content")?.to_string()),
                    kind: file_kind(r)?,
                }
            })
        })
        .collect::<ToolResult<Vec<_>>>()?;
    let result = repo
        .resolve(version, resolutions, provenance(args)?)
        .await?;
    Ok(json::version_result(&result))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_edit_op() {
        let args = Args::new(json!({
            "edits": [
                {"op": "set", "path": "a.txt", "content": "x", "kind": "executable"},
                {"op": "delete", "path": "b.txt"},
                {"op": "patch", "path": "c.txt", "unified": "--- a\n+++ b\n"},
            ]
        }))
        .unwrap();
        let edits = parse_edits(&args).unwrap();
        assert!(matches!(
            &edits[0],
            Edit::Set {
                kind: FileKind::Executable,
                ..
            }
        ));
        assert!(matches!(&edits[1], Edit::Delete { .. }));
        assert!(matches!(&edits[2], Edit::Patch { .. }));
    }

    #[test]
    fn provenance_names_the_tool() {
        let args = Args::new(json!({ "model": "m", "session": "s" })).unwrap();
        let p = provenance(&args).unwrap();
        assert_eq!(p.tool.as_deref(), Some(TOOL_NAME));
        assert_eq!(p.model.as_deref(), Some("m"));
        assert_eq!(p.session.as_deref(), Some("s"));
    }
}
