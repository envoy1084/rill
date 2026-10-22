//! `rill ws`: server-side workspaces: create, list, show, handoff, ops,
//! delete.

use clap::{Args, Subcommand};
use rill_mcp::json;
use rill_sdk::{Root, WorkspaceFilter};
use rill_types::{Principal, RepoPathBuf, SparseProfile, Workspace, WorkspaceId};
use serde_json::json as j;

use crate::commands::root_arg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{short, when, Out, Table};
use crate::Result;

/// `rill ws <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum WsCmd {
    /// Create a workspace.
    Create(CreateArgs),
    /// List workspaces.
    List(ListArgs),
    /// Show a workspace (default: the checkout's).
    Show(WsArg),
    /// Hand a workspace to another principal.
    Handoff(HandoffArgs),
    /// Show a workspace's operation log.
    Ops(OpsArgs),
    /// Delete a workspace.
    Delete(WsArg),
}

/// A command taking an optional workspace id.
#[derive(Args, Debug)]
pub struct WsArg {
    /// Workspace id (default: the checkout's workspace).
    pub workspace: Option<String>,
}

/// `rill ws create`.
#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Workspace name.
    pub name: String,
    /// Base root (default trunk).
    #[arg(long, value_name = "ROOT")]
    pub at: Option<String>,
    /// Sparse include prefixes (repeatable).
    #[arg(long, value_name = "PREFIX")]
    pub sparse: Vec<String>,
}

/// `rill ws list`.
#[derive(Args, Debug)]
pub struct ListArgs {
    /// Page size.
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
}

/// `rill ws handoff`.
#[derive(Args, Debug)]
pub struct HandoffArgs {
    /// Workspace id.
    pub workspace: String,
    /// Principal to hand to: `name`, `agent:name` or `svc:name`.
    pub principal: String,
}

/// `rill ws ops`.
#[derive(Args, Debug)]
pub struct OpsArgs {
    /// Workspace id (default: the checkout's workspace).
    pub workspace: Option<String>,
    /// Maximum operations.
    #[arg(long, short = 'n', default_value_t = 20)]
    pub limit: u32,
}

fn parse_ws(text: &str) -> Result<WorkspaceId> {
    text.parse()
        .map_err(|e| CliError::usage(format!("invalid workspace id {text:?}: {e}")))
}

/// Resolve a workspace argument, defaulting to the checkout's.
fn ws_or_checkout(ctx: &mut Ctx<'_>, arg: Option<&str>) -> Result<WorkspaceId> {
    match arg {
        Some(w) => parse_ws(w),
        None => ctx
            .checkout_meta()
            .and_then(|m| m.workspace)
            .ok_or_else(|| CliError::usage("no workspace in this checkout; pass a workspace id")),
    }
}

/// Parse `name`, `agent:name` or `svc:name` into a principal.
pub fn parse_principal(text: &str) -> Principal {
    match text.split_once(':') {
        Some(("agent", name)) => Principal::agent(name),
        Some(("svc", name)) | Some(("service", name)) => Principal::service(name),
        Some(("human", name)) => Principal::human(name),
        _ => Principal::human(text),
    }
}

fn print_ws(out: &mut Out<'_>, w: &Workspace) -> std::io::Result<()> {
    out.kv(&[
        ("id", out.id(&w.id.to_string())),
        ("name", w.name.clone()),
        ("owner", w.owner.to_string()),
        ("base", short(&w.base)),
        ("working change", w.working_change.to_string()),
        (
            "snapshot",
            w.snapshot.map(|s| short(&s)).unwrap_or_default(),
        ),
        (
            "sparse",
            if w.sparse.is_full() {
                "full".into()
            } else {
                w.sparse
                    .include
                    .iter()
                    .map(|p| p.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            },
        ),
        (
            "head op",
            w.head_op.map(|o| o.to_string()).unwrap_or_default(),
        ),
        ("updated", when(&w.updated_at)),
    ])
}

/// Run a workspace subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: WsCmd) -> Result<()> {
    match cmd {
        WsCmd::Create(args) => create(ctx, args).await,
        WsCmd::List(args) => list(ctx, args).await,
        WsCmd::Show(args) => show(ctx, args).await,
        WsCmd::Handoff(args) => handoff(ctx, args).await,
        WsCmd::Ops(args) => ops(ctx, args).await,
        WsCmd::Delete(args) => delete(ctx, args).await,
    }
}

async fn create(ctx: &mut Ctx<'_>, args: CreateArgs) -> Result<()> {
    let workspaces = ctx.repo()?.workspaces();
    let base: Root = root_arg(args.at.as_deref())?;
    let sparse = SparseProfile {
        include: args
            .sparse
            .iter()
            .map(RepoPathBuf::new)
            .collect::<std::result::Result<_, _>>()?,
        exclude: Vec::new(),
    };
    let op = workspaces.create(&args.name, Some(base), sparse).await?;
    ctx.emit(json::workspace_op(&op), |out| {
        out.line(format!(
            "created workspace {} ({})",
            op.workspace.name,
            out.id(&op.workspace.id.to_string())
        ))
    })
}

async fn list(ctx: &mut Ctx<'_>, args: ListArgs) -> Result<()> {
    let workspaces = ctx.repo()?.workspaces();
    let page = workspaces
        .list(WorkspaceFilter {
            owner: None,
            limit: args.limit,
            page_token: None,
        })
        .await?;
    ctx.emit(json::page(&page, json::workspace), |out| {
        if page.items.is_empty() {
            return out.line(out.dim("no workspaces"));
        }
        let mut table = Table::new(&["ID", "NAME", "OWNER", "BASE", "UPDATED"]);
        for w in &page.items {
            table.row(vec![
                out.id(&w.id.to_string()),
                w.name.clone(),
                w.owner.to_string(),
                short(&w.base),
                when(&w.updated_at),
            ]);
        }
        out.table(table)
    })
}

async fn show(ctx: &mut Ctx<'_>, args: WsArg) -> Result<()> {
    let id = ws_or_checkout(ctx, args.workspace.as_deref())?;
    let ws = ctx.repo()?.workspaces().get(id).await?;
    ctx.emit(json::workspace(&ws), |out| print_ws(out, &ws))
}

async fn handoff(ctx: &mut Ctx<'_>, args: HandoffArgs) -> Result<()> {
    let id = parse_ws(&args.workspace)?;
    let to = parse_principal(&args.principal);
    let op = ctx.repo()?.workspaces().handoff(id, to.clone()).await?;
    ctx.emit(json::workspace_op(&op), |out| {
        out.line(format!("handed {} to {to}", out.id(&id.to_string())))
    })
}

async fn ops(ctx: &mut Ctx<'_>, args: OpsArgs) -> Result<()> {
    let id = ws_or_checkout(ctx, args.workspace.as_deref())?;
    let ops = ctx
        .repo()?
        .workspaces()
        .operations(id, args.limit, None)
        .await?
        .items;
    ctx.emit(
        j!({ "workspace": id.to_string(), "operations": ops.iter().map(json::operation).collect::<Vec<_>>() }),
        |out| {
            if ops.is_empty() {
                return out.line(out.dim("no operations"));
            }
            let mut table = Table::new(&["OP", "KIND", "ACTOR", "BASE", "WHEN"]);
            for o in &ops {
                let kind = json::operation(o)["op"]["kind"]
                    .as_str()
                    .unwrap_or("?")
                    .to_string();
                table.row(vec![
                    out.id(&o.id.to_string()),
                    kind,
                    o.actor.to_string(),
                    short(&o.resulting.base),
                    when(&o.at),
                ]);
            }
            out.table(table)
        },
    )
}

async fn delete(ctx: &mut Ctx<'_>, args: WsArg) -> Result<()> {
    let id = ws_or_checkout(ctx, args.workspace.as_deref())?;
    ctx.repo()?.workspaces().delete(id).await?;
    ctx.emit(j!({ "deleted": id.to_string() }), |out| {
        out.line(format!("deleted workspace {}", out.id(&id.to_string())))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn principal_prefixes() {
        assert_eq!(parse_principal("alice"), Principal::human("alice"));
        assert_eq!(parse_principal("agent:bot"), Principal::agent("bot"));
        assert_eq!(parse_principal("svc:lander"), Principal::service("lander"));
    }
}
