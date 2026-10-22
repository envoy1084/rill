//! `rill intent`: create, list, show, update, link, similar.

use clap::{Args, Subcommand};
use rill_mcp::json;
use rill_sdk::{FieldUpdate, IntentFilter, IntentUpdate, NewIntent};
use rill_types::{Intent, IntentId, IntentKind, IntentStatus};
use serde_json::json as j;

use crate::commands::change_ref;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{when, Out, Table};
use crate::Result;

/// `rill intent <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum IntentCmd {
    /// Create an intent.
    Create(CreateArgs),
    /// List intents.
    List(ListArgs),
    /// Show one intent.
    Show(IdArg),
    /// Update title, body, status or parent.
    Update(UpdateArgs),
    /// Link a change to an intent.
    Link(LinkArgs),
    /// Find intents similar to some text.
    Similar(SimilarArgs),
}

/// A command taking an intent id.
#[derive(Args, Debug)]
pub struct IdArg {
    /// Intent id.
    pub id: String,
}

/// `rill intent create`.
#[derive(Args, Debug)]
pub struct CreateArgs {
    /// Title.
    pub title: String,
    /// Kind: feature, bug, refactor, chore, experiment, system.
    #[arg(long, default_value = "feature")]
    pub kind: String,
    /// Body text.
    #[arg(long)]
    pub body: Option<String>,
    /// Parent intent id.
    #[arg(long)]
    pub parent: Option<String>,
}

/// `rill intent list`.
#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only this status: open, in_progress, done, abandoned.
    #[arg(long)]
    pub status: Option<String>,
    /// Only this kind.
    #[arg(long)]
    pub kind: Option<String>,
    /// Only children of this intent.
    #[arg(long)]
    pub parent: Option<String>,
    /// Page size.
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
}

/// `rill intent update`.
#[derive(Args, Debug)]
pub struct UpdateArgs {
    /// Intent id.
    pub id: String,
    /// New title.
    #[arg(long)]
    pub title: Option<String>,
    /// New body.
    #[arg(long)]
    pub body: Option<String>,
    /// New status: open, in_progress, done, abandoned.
    #[arg(long)]
    pub status: Option<String>,
    /// New parent intent id.
    #[arg(long, conflicts_with = "no_parent")]
    pub parent: Option<String>,
    /// Detach from the parent.
    #[arg(long)]
    pub no_parent: bool,
}

/// `rill intent link`.
#[derive(Args, Debug)]
pub struct LinkArgs {
    /// Intent id.
    pub intent: String,
    /// Change id or version hash.
    pub change: String,
}

/// `rill intent similar`.
#[derive(Args, Debug)]
pub struct SimilarArgs {
    /// Text to match (treated as the title; use --body for more).
    pub text: String,
    /// Body text to match.
    #[arg(long, default_value = "")]
    pub body: String,
    /// Maximum hits.
    #[arg(long, default_value_t = 10)]
    pub limit: u32,
}

fn parse_id(text: &str) -> Result<IntentId> {
    text.parse()
        .map_err(|e| CliError::usage(format!("invalid intent id {text:?}: {e}")))
}

fn parse_kind(text: &str) -> Result<IntentKind> {
    Ok(match text.to_ascii_lowercase().as_str() {
        "feature" => IntentKind::Feature,
        "bug" => IntentKind::Bug,
        "refactor" => IntentKind::Refactor,
        "chore" => IntentKind::Chore,
        "experiment" => IntentKind::Experiment,
        "system" => IntentKind::System,
        other => return Err(CliError::usage(format!("unknown intent kind {other:?}"))),
    })
}

fn parse_status(text: &str) -> Result<IntentStatus> {
    Ok(match text.to_ascii_lowercase().replace('-', "_").as_str() {
        "open" => IntentStatus::Open,
        "in_progress" => IntentStatus::InProgress,
        "done" => IntentStatus::Done,
        "abandoned" => IntentStatus::Abandoned,
        other => return Err(CliError::usage(format!("unknown intent status {other:?}"))),
    })
}

fn print_intent(out: &mut Out<'_>, i: &Intent) -> std::io::Result<()> {
    out.kv(&[
        ("id", out.id(&i.id.to_string())),
        ("title", out.bold(&i.title)),
        ("kind", format!("{:?}", i.kind).to_ascii_lowercase()),
        ("status", format!("{:?}", i.status).to_ascii_lowercase()),
        ("owner", i.owner.to_string()),
        (
            "parent",
            i.parent.map(|p| p.to_string()).unwrap_or_default(),
        ),
        (
            "risk",
            i.risk_override.map(|r| r.to_string()).unwrap_or_default(),
        ),
        (
            "changes",
            i.changes
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" "),
        ),
        ("updated", when(&i.updated_at)),
    ])?;
    if !i.body.is_empty() {
        out.blank()?;
        for line in i.body.lines() {
            out.line(format!("    {line}"))?;
        }
    }
    Ok(())
}

/// Run an intent subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: IntentCmd) -> Result<()> {
    match cmd {
        IntentCmd::Create(args) => create(ctx, args).await,
        IntentCmd::List(args) => list(ctx, args).await,
        IntentCmd::Show(args) => show(ctx, args).await,
        IntentCmd::Update(args) => update(ctx, args).await,
        IntentCmd::Link(args) => link(ctx, args).await,
        IntentCmd::Similar(args) => similar(ctx, args).await,
    }
}

async fn create(ctx: &mut Ctx<'_>, args: CreateArgs) -> Result<()> {
    let intents = ctx.repo()?.intents();
    let mut new = NewIntent::new(args.title, parse_kind(&args.kind)?);
    if let Some(body) = args.body {
        new = new.with_body(body);
    }
    new.parent = args.parent.as_deref().map(parse_id).transpose()?;
    let intent = intents.create(new).await?;
    ctx.emit(json::intent(&intent), |out| {
        out.line(format!(
            "created intent {} {}",
            out.id(&intent.id.to_string()),
            intent.title
        ))
    })
}

async fn list(ctx: &mut Ctx<'_>, args: ListArgs) -> Result<()> {
    let intents = ctx.repo()?.intents();
    let filter = IntentFilter {
        status: args
            .status
            .as_deref()
            .map(parse_status)
            .transpose()?
            .into_iter()
            .collect(),
        kind: args
            .kind
            .as_deref()
            .map(parse_kind)
            .transpose()?
            .into_iter()
            .collect(),
        owner: None,
        parent: args.parent.as_deref().map(parse_id).transpose()?,
        limit: args.limit,
        page_token: None,
    };
    let page = intents.list(filter).await?;
    ctx.emit(json::page(&page, json::intent), |out| {
        if page.items.is_empty() {
            return out.line(out.dim("no intents"));
        }
        let mut table = Table::new(&["ID", "KIND", "STATUS", "OWNER", "CHANGES", "TITLE"]);
        for i in &page.items {
            table.row(vec![
                out.id(&i.id.to_string()),
                format!("{:?}", i.kind).to_ascii_lowercase(),
                format!("{:?}", i.status).to_ascii_lowercase(),
                i.owner.to_string(),
                i.changes.len().to_string(),
                i.title.clone(),
            ]);
        }
        out.table(table)
    })
}

async fn show(ctx: &mut Ctx<'_>, args: IdArg) -> Result<()> {
    let intents = ctx.repo()?.intents();
    let intent = intents.get(parse_id(&args.id)?).await?;
    ctx.emit(json::intent(&intent), |out| print_intent(out, &intent))
}

async fn update(ctx: &mut Ctx<'_>, args: UpdateArgs) -> Result<()> {
    let intents = ctx.repo()?.intents();
    let update = IntentUpdate {
        title: args.title,
        body: args.body,
        status: args.status.as_deref().map(parse_status).transpose()?,
        parent: match (&args.parent, args.no_parent) {
            (Some(p), _) => FieldUpdate::Set(parse_id(p)?),
            (None, true) => FieldUpdate::Clear,
            (None, false) => FieldUpdate::Keep,
        },
        risk_override: FieldUpdate::Keep,
    };
    let intent = intents.update(parse_id(&args.id)?, update).await?;
    ctx.emit(json::intent(&intent), |out| print_intent(out, &intent))
}

async fn link(ctx: &mut Ctx<'_>, args: LinkArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let change = change_ref(&args.change)?.id(&repo).await?;
    let intent = repo
        .intents()
        .link_change(parse_id(&args.intent)?, change)
        .await?;
    ctx.emit(json::intent(&intent), |out| {
        out.line(format!(
            "linked {} to intent {}",
            out.id(&change.to_string()),
            out.id(&intent.id.to_string())
        ))
    })
}

async fn similar(ctx: &mut Ctx<'_>, args: SimilarArgs) -> Result<()> {
    let intents = ctx.repo()?.intents();
    let hits = intents
        .find_similar(&args.text, &args.body, args.limit)
        .await?;
    ctx.emit(
        j!({ "hits": hits.iter().map(json::scored_intent).collect::<Vec<_>>() }),
        |out| {
            if hits.is_empty() {
                return out.line(out.dim("no similar intents"));
            }
            let mut table = Table::new(&["SCORE", "ID", "STATUS", "TITLE"]);
            for h in &hits {
                table.row(vec![
                    format!("{:.2}", h.score),
                    out.id(&h.intent.id.to_string()),
                    format!("{:?}", h.intent.status).to_ascii_lowercase(),
                    h.intent.title.clone(),
                ]);
            }
            out.table(table)
        },
    )
}
