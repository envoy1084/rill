//! `rill repo`: list, create, info.

use clap::{Args, Subcommand};
use rill_mcp::json;

use crate::context::Ctx;
use crate::output::{when, Table};
use crate::Result;

/// `rill repo <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum RepoCmd {
    /// List repositories on the server.
    List(ListArgs),
    /// Create a repository.
    Create(CreateArgs),
    /// Show one repository (the current one by default).
    Info,
}

/// `rill repo list`.
#[derive(Args, Debug)]
pub struct ListArgs {
    /// Maximum repositories to show.
    #[arg(long, default_value_t = 100)]
    pub limit: u32,
}

/// `rill repo create`.
#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Repository name: `org/name`, or a name in the default org.
    pub name: String,
    /// Free-form description.
    #[arg(long)]
    pub description: Option<String>,
    /// Reject paths Windows cannot create (CON, aux.txt, a:b, ...).
    #[arg(long)]
    pub windows_safe: bool,
}

/// Run a repo subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: RepoCmd) -> Result<()> {
    match cmd {
        RepoCmd::List(args) => list(ctx, args).await,
        RepoCmd::Create(args) => create(ctx, args).await,
        RepoCmd::Info => info(ctx).await,
    }
}

async fn list(ctx: &mut Ctx<'_>, args: ListArgs) -> Result<()> {
    let page = ctx.client()?.list_repos(args.limit, None).await?;
    ctx.emit(json::page(&page, json::repo_info), |out| {
        if page.items.is_empty() {
            return out.line("no repositories");
        }
        let mut table = Table::new(&["NAME", "TRUNK", "CREATED", "ID", "DESCRIPTION"]);
        for r in &page.items {
            table.row(vec![
                format!("{}/{}", r.org, r.name),
                format!("trunk@{}", r.trunk_head_seq.get()),
                when(&r.created_at),
                r.id.to_string(),
                r.description.clone().unwrap_or_default(),
            ]);
        }
        out.table(table)?;
        if page.has_more() {
            out.line(out.dim("(more repositories; raise --limit)"))?;
        }
        Ok(())
    })
}

async fn create(ctx: &mut Ctx<'_>, args: CreateArgs) -> Result<()> {
    let repo = ctx
        .client()?
        .create_repo_with_op(
            rill_types::OperationId::new(),
            &args.name,
            args.description,
            args.windows_safe,
        )
        .await?;
    ctx.emit(json::repo_info(&repo), |out| {
        out.line(format!(
            "created repository {} ({})",
            out.bold(&format!("{}/{}", repo.org, repo.name)),
            repo.id
        ))
    })
}

async fn info(ctx: &mut Ctx<'_>) -> Result<()> {
    let repo = ctx.repo()?.info().await?;
    ctx.emit(json::repo_info(&repo), |out| {
        out.kv(&[
            ("name", repo.name.clone()),
            ("org", repo.org.clone()),
            ("id", repo.id.to_string()),
            ("description", repo.description.clone().unwrap_or_default()),
            ("trunk head", format!("trunk@{}", repo.trunk_head_seq.get())),
            ("created", when(&repo.created_at)),
        ])
    })
}
