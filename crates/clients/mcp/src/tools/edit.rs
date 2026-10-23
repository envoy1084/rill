//! `rill_edit`: create a change from edits, or amend the current one.
//! Without `change` it starts a new change on `base`; with one (or the
//! session's) it adds a version carrying the change's earlier edits.

use bytes::Bytes;
use rill_sdk::{ApplyOptions, Edit, EditBase, RepoClient, Root};
use rill_types::{EntryKind, IntentId, RepoPathBuf};
use serde_json::{json, Value};

use super::args::Args;
use super::hints::next_step;
use super::provenance::{file_kind, provenance};
use super::schema::{boolean, provenance_props, root, string, string_enum, Props};
use super::{ToolContext, ToolDef};
use crate::error::{ToolError, ToolResult};
use crate::json;

/// Paths listed in the reply; `files_changed` counts them all.
const MAX_FILES_SHOWN: usize = 200;

const KINDS: &[&str] = &["regular", "executable", "symlink"];

fn edits_schema() -> Value {
    json!({
        "type": "array",
        "description": "Edits, applied in order",
        "items": {
            "type": "object",
            "properties": {
                "op": string_enum("set: write content; delete; patch: apply a unified diff; move: rename to `to`", &["set", "delete", "patch", "move"]),
                "path": string("Repository path"),
                "content": string("op=set: the whole new content"),
                "kind": string_enum("op=set: file kind (default regular)", KINDS),
                "unified": string("op=patch: unified diff against the current content"),
                "to": string("op=move: the new path"),
            },
            "required": ["op", "path"],
            "additionalProperties": false,
        }
    })
}

/// The tool's definition.
pub fn definition() -> ToolDef {
    ToolDef {
        name: "rill_edit",
        description: "Edit files. With no current change this starts one on `base` (default trunk) and makes it the session's change; otherwise it amends that change (or `change`), keeping its earlier edits. Every call returns the change id; pass it as `change` from stateless clients. Pass `model` and `session` so the change is attributable.",
        input_schema: provenance_props(Props::repo()
            .req("edits", edits_schema())
            .change()
            .opt("new", boolean("Start a new change even if the session has one"))
            .opt("base", root("Where a new change starts (default trunk)"))
            .opt("message", string("Change message (new changes default to `wip`; amends keep theirs)"))
            .opt("intent", string("Intent id the change serves"))
            )
            .build(),
    }
}

/// Run the tool.
pub async fn call(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let repo = ctx.repo(args)?;
    let amend = match args.bool_or("new", false)? {
        true => None,
        false => ctx.opt_change(args)?,
    };
    let (base, read_at, message) = match amend {
        Some(id) => {
            let info = repo.current_version(id).await?;
            let message = args.opt_string("message")?.unwrap_or(info.message);
            (EditBase::Change(id), Root::Version(info.hash), message)
        }
        None => {
            let root = args.root_or_trunk("base")?;
            let message = args.opt_string("message")?.unwrap_or_else(|| "wip".into());
            (EditBase::Root(root.clone()), root, message)
        }
    };
    let edits = parse_edits(&repo, &read_at, args).await?;
    let mut opts = ApplyOptions::message(message).with_provenance(provenance(args)?);
    if let Some(intent) = args.opt_parse::<IntentId>("intent")? {
        opts = opts.with_intent(intent);
    }
    let result = repo.apply_edits(base, edits, opts).await?;
    let change = result.version.change_id;
    ctx.work_on(args, change)?;
    let conflicts: Vec<&str> = result
        .version
        .conflicted_paths()
        .map(|p| p.as_str())
        .collect();
    Ok(json!({
        "change": change.to_string(),
        "version": result.hash.to_hex(),
        "amended": amend.is_some(),
        "message": result.version.summary(),
        "files_changed": result.version.file_changes.len(),
        "files": result.version.file_changes.iter().take(MAX_FILES_SHOWN)
            .map(|(p, c)| json!({ "path": p.as_str(), "op": json::file_change(c)["op"] }))
            .collect::<Vec<_>>(),
        "conflicts": conflicts,
        "next": next_step(conflicts.len(), None, None),
    }))
}

async fn parse_edits(repo: &RepoClient, at: &Root, args: &Args) -> ToolResult<Vec<Edit>> {
    let mut edits = Vec::new();
    for e in args.objects("edits")? {
        let path = e.path("path")?;
        match e.str("op")? {
            "set" => edits.push(Edit::Set {
                path,
                bytes: Bytes::from(e.str("content")?.to_string()),
                kind: file_kind(&e)?,
            }),
            "delete" => edits.push(Edit::Delete { path }),
            "patch" => edits.push(Edit::Patch {
                path,
                unified: e.str("unified")?.to_string(),
            }),
            "move" => {
                let to = e.path("to")?;
                edits.extend(moved(repo, at, path, to).await?);
            }
            other => return Err(ToolError::invalid(format!("unknown edit op {other:?}"))),
        }
    }
    Ok(edits)
}

/// A move as a write of the old content at the new path plus a delete.
async fn moved(
    repo: &RepoClient,
    at: &Root,
    from: RepoPathBuf,
    to: RepoPathBuf,
) -> ToolResult<[Edit; 2]> {
    let Some(EntryKind::File { content, kind }) = repo.resolve_path(at, from.as_path()).await?
    else {
        return Err(ToolError::invalid(format!(
            "move: {from} is not a file at {at}"
        )));
    };
    let bytes = repo.get_blob(&content).await?;
    Ok([
        Edit::Set {
            path: to,
            bytes,
            kind,
        },
        Edit::Delete { path: from },
    ])
}
