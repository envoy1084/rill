//! Workspace tools: create, diff and handoff.

use rill_sdk::{DiffFormat, Root};
use rill_types::{Principal, SparseProfile, WorkspaceId};
use serde_json::{json, Value};

use super::args::Args;
use super::schema::{boolean, integer, root, string, string_array, string_enum, Props};
use super::{ToolContext, ToolDef};
use crate::error::ToolResult;
use crate::json;

/// Tool definitions in this group.
pub fn definitions() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "rill_workspace_create",
            description: "Create a server-side workspace based on a root, optionally sparse.",
            input_schema: Props::repo()
                .req("name", string("Workspace name"))
                .opt("base", root("Base root (default trunk)"))
                .opt("include", string_array("Sparse include prefixes"))
                .opt("exclude", string_array("Sparse exclude prefixes"))
                .build(),
        },
        ToolDef {
            name: "rill_workspace_diff",
            description: "Diff a workspace's latest snapshot against its base.",
            input_schema: Props::repo()
                .req("workspace", string("Workspace id"))
                .opt("paths", string_array("Restrict to these paths or prefixes"))
                .opt(
                    "unified",
                    boolean("Return unified diff text instead of structured hunks"),
                )
                .opt(
                    "context_lines",
                    integer("Context lines per hunk (default 3)"),
                )
                .build(),
        },
        ToolDef {
            name: "rill_workspace_handoff",
            description: "Hand a workspace to another principal (a human or another agent).",
            input_schema: Props::repo()
                .req("workspace", string("Workspace id"))
                .req("to", string("Principal name to hand the workspace to"))
                .opt(
                    "to_kind",
                    string_enum(
                        "Kind of that principal (default human)",
                        &["human", "agent", "service"],
                    ),
                )
                .build(),
        },
    ]
}

/// Dispatch a call in this group.
pub async fn call(ctx: &ToolContext, name: &str, args: &Args) -> Option<ToolResult<Value>> {
    Some(match name {
        "rill_workspace_create" => create(ctx, args).await,
        "rill_workspace_diff" => diff(ctx, args).await,
        "rill_workspace_handoff" => handoff(ctx, args).await,
        _ => return None,
    })
}

async fn create(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let workspaces = ctx.repo(args)?.workspaces();
    let sparse = SparseProfile {
        include: args.opt_paths("include")?,
        exclude: args.opt_paths("exclude")?,
    };
    let base = args.opt_parse::<Root>("base")?;
    let op = workspaces.create(args.str("name")?, base, sparse).await?;
    Ok(json::workspace_op(&op))
}

async fn diff(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let workspaces = ctx.repo(args)?.workspaces();
    let format = if args.bool_or("unified", false)? {
        DiffFormat::Unified
    } else {
        DiffFormat::Structured
    };
    let files = workspaces
        .diff(
            args.parse::<WorkspaceId>("workspace")?,
            args.opt_paths("paths")?,
            format,
            args.limit("context_lines", 3)?,
        )
        .await?;
    Ok(json!({ "files": files.iter().map(json::file_diff).collect::<Vec<_>>() }))
}

/// Build a principal from a name and kind.
pub fn principal(name: &str, kind: Option<&str>) -> ToolResult<Principal> {
    Ok(match kind {
        None | Some("human") => Principal::human(name),
        Some("agent") => Principal::agent(name),
        Some("service") => Principal::service(name),
        Some(other) => {
            return Err(crate::error::ToolError::invalid(format!(
                "unknown principal kind {other:?}"
            )))
        }
    })
}

async fn handoff(ctx: &ToolContext, args: &Args) -> ToolResult<Value> {
    let workspaces = ctx.repo(args)?.workspaces();
    let to = principal(args.str("to")?, args.opt_str("to_kind"))?;
    let op = workspaces
        .handoff(args.parse::<WorkspaceId>("workspace")?, to)
        .await?;
    Ok(json::workspace_op(&op))
}
