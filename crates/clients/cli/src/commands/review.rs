//! `rill submit` and `rill review`: show, list, vote, comment, threads,
//! resolve-thread, evaluate, abandon.

use clap::{Args, Subcommand};
use rill_mcp::json;
use rill_sdk::{NewComment, RepoClient, ReviewFilter};
use rill_types::{ChangeId, PrincipalId, RepoPathBuf, ReviewState, ReviewStatus, ThreadAnchor};
use serde_json::json as j;

use crate::commands::change_ref;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{short, when, Out, Table};
use crate::Result;

/// `rill submit`.
#[derive(Args, Debug)]
pub struct SubmitArgs {
    /// Change to submit (default: the checkout's working change).
    pub change: Option<String>,
}

/// `rill review <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ReviewCmd {
    /// Show a review: status, votes, threads, risk.
    Show(ChangeArg),
    /// List reviews.
    List(ListArgs),
    /// Vote on a label.
    Vote(VoteArgs),
    /// Open a thread.
    Comment(CommentArgs),
    /// List threads.
    Threads(ChangeArg),
    /// Resolve (or reopen) a thread.
    ResolveThread(ResolveThreadArgs),
    /// Evaluate the submit policy and print the reasons.
    Evaluate(ChangeArg),
    /// Abandon a change.
    Abandon(AbandonArgs),
}

/// A command taking one change.
#[derive(Args, Debug)]
pub struct ChangeArg {
    /// Change id or version hash (default: the checkout's working change).
    pub change: Option<String>,
}

/// `rill review list`.
#[derive(Args, Debug)]
pub struct ListArgs {
    /// Only this status (open, landable, landing, landed, abandoned).
    #[arg(long)]
    pub status: Option<String>,
    /// Only reviews owned by this principal id.
    #[arg(long)]
    pub owner: Option<String>,
    /// Page size.
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
}

/// `rill review vote`.
#[derive(Args, Debug)]
pub struct VoteArgs {
    /// Change id or version hash.
    pub change: String,
    /// Label name (Review, Verified, Owner, ...).
    pub label: String,
    /// Vote value, e.g. +2 or -1.
    #[arg(allow_hyphen_values = true)]
    pub value: String,
    /// Reason for the vote.
    #[arg(short, long)]
    pub message: Option<String>,
}

/// `rill review comment`.
#[derive(Args, Debug)]
pub struct CommentArgs {
    /// Change id or version hash.
    pub change: String,
    /// Comment text.
    #[arg(short, long)]
    pub message: String,
    /// Anchor to this file.
    #[arg(long)]
    pub path: Option<String>,
    /// Anchor to this line (requires --path).
    #[arg(long)]
    pub line: Option<u32>,
}

/// `rill review resolve-thread`.
#[derive(Args, Debug)]
pub struct ResolveThreadArgs {
    /// Change id or version hash.
    pub change: String,
    /// Thread id.
    pub thread: String,
    /// Reopen instead of resolving.
    #[arg(long)]
    pub reopen: bool,
}

/// `rill review abandon`.
#[derive(Args, Debug)]
pub struct AbandonArgs {
    /// Change id or version hash.
    pub change: String,
    /// Why.
    #[arg(short, long)]
    pub message: Option<String>,
}

/// Resolve a change argument, defaulting to the checkout's working change.
pub async fn change_or_working(
    ctx: &mut Ctx<'_>,
    change: Option<&str>,
    repo: &RepoClient,
) -> Result<ChangeId> {
    match change {
        Some(c) => change_ref(c)?.id(repo).await,
        None => ctx
            .checkout_meta()
            .and_then(|m| m.working_change)
            .ok_or_else(|| CliError::usage("no working change in this checkout; pass a change id")),
    }
}

fn parse_status(s: &str) -> Result<ReviewStatus> {
    Ok(match s.to_ascii_lowercase().as_str() {
        "open" => ReviewStatus::Open,
        "landable" => ReviewStatus::Landable,
        "landing" => ReviewStatus::Landing,
        "landed" => ReviewStatus::Landed,
        "abandoned" => ReviewStatus::Abandoned,
        other => return Err(CliError::usage(format!("unknown review status {other:?}"))),
    })
}

fn status_text(out: &Out<'_>, s: ReviewStatus) -> String {
    let name = format!("{s:?}").to_ascii_lowercase();
    match s {
        ReviewStatus::Landable | ReviewStatus::Landed => out.good(&name),
        ReviewStatus::Abandoned => out.bad(&name),
        _ => out.warn(&name),
    }
}

/// Run `rill submit`.
pub async fn submit(ctx: &mut Ctx<'_>, args: SubmitArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let state = repo.reviews().submit(id).await?;
    ctx.emit(json::review_state(&state), |out| {
        out.line(format!(
            "submitted {} for review ({})",
            out.id(&id.to_string()),
            status_text(out, state.status)
        ))
    })
}

/// Run a review subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: ReviewCmd) -> Result<()> {
    match cmd {
        ReviewCmd::Show(args) => show(ctx, args).await,
        ReviewCmd::List(args) => list(ctx, args).await,
        ReviewCmd::Vote(args) => vote(ctx, args).await,
        ReviewCmd::Comment(args) => comment(ctx, args).await,
        ReviewCmd::Threads(args) => threads(ctx, args).await,
        ReviewCmd::ResolveThread(args) => resolve_thread(ctx, args).await,
        ReviewCmd::Evaluate(args) => evaluate(ctx, args).await,
        ReviewCmd::Abandon(args) => abandon(ctx, args).await,
    }
}

fn print_state(out: &mut Out<'_>, r: &ReviewState) -> std::io::Result<()> {
    out.kv(&[
        ("change", out.id(&r.change_id.to_string())),
        ("status", status_text(out, r.status)),
        ("owner", r.owner.to_string()),
        ("risk", format!("{} {}", r.risk, r.risk_reasons.join("; "))),
        (
            "version",
            r.current_version().map(|h| short(&h)).unwrap_or_default(),
        ),
        ("versions", r.versions.len().to_string()),
        (
            "landed",
            r.landed_seq
                .map(|s| format!("trunk@{}", s.get()))
                .unwrap_or_default(),
        ),
        (
            "intent",
            r.intent.map(|i| i.to_string()).unwrap_or_default(),
        ),
        (
            "stack",
            r.stack
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join(" "),
        ),
        (
            "threads",
            format!(
                "{} ({} unresolved)",
                r.threads.len(),
                r.unresolved_threads()
            ),
        ),
        ("updated", when(&r.updated_at)),
    ])?;
    let votes: Vec<_> = r.current_votes().collect();
    if !votes.is_empty() {
        out.blank()?;
        let mut table = Table::new(&["LABEL", "VALUE", "VOTER", "WHEN", "REASON"]);
        for v in votes {
            let value = if v.value > 0 {
                out.good(&format!("+{}", v.value))
            } else if v.value < 0 {
                out.bad(&v.value.to_string())
            } else {
                "0".to_string()
            };
            table.row(vec![
                v.label.clone(),
                value,
                v.voter.to_string(),
                when(&v.voted_at),
                v.reason.clone().unwrap_or_default(),
            ]);
        }
        out.table(table)?;
    }
    Ok(())
}

async fn show(ctx: &mut Ctx<'_>, args: ChangeArg) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let info = repo.reviews().get(id).await?;
    ctx.emit(json::review_info(&info), |out| {
        print_state(out, &info.review)
    })
}

async fn list(ctx: &mut Ctx<'_>, args: ListArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let filter = ReviewFilter {
        status: args
            .status
            .as_deref()
            .map(parse_status)
            .transpose()?
            .into_iter()
            .collect(),
        owner: args
            .owner
            .as_deref()
            .map(|o| {
                o.parse::<PrincipalId>()
                    .map_err(|e| CliError::usage(format!("invalid principal id {o:?}: {e}")))
            })
            .transpose()?,
        intent: None,
        limit: args.limit,
        page_token: None,
    };
    let page = repo.reviews().list(filter).await?;
    ctx.emit(json::page(&page, json::review_state), |out| {
        if page.items.is_empty() {
            return out.line(out.dim("no reviews"));
        }
        let mut table = Table::new(&["CHANGE", "STATUS", "RISK", "OWNER", "VERSION", "UPDATED"]);
        for r in &page.items {
            table.row(vec![
                out.id(&r.change_id.to_string()),
                status_text(out, r.status),
                r.risk.to_string(),
                r.owner.to_string(),
                r.current_version().map(|h| short(&h)).unwrap_or_default(),
                when(&r.updated_at),
            ]);
        }
        out.table(table)
    })
}

async fn vote(ctx: &mut Ctx<'_>, args: VoteArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_ref(&args.change)?.id(&repo).await?;
    let value: i8 = args
        .value
        .trim_start_matches('+')
        .parse()
        .map_err(|_| CliError::usage(format!("invalid vote value {:?}", args.value)))?;
    let state = repo
        .reviews()
        .vote(id, &args.label, value, args.message, None)
        .await?;
    ctx.emit(json::review_state(&state), |out| {
        out.line(format!(
            "voted {} {:+} on {} ({})",
            args.label,
            value,
            out.id(&id.to_string()),
            status_text(out, state.status)
        ))
    })
}

async fn comment(ctx: &mut Ctx<'_>, args: CommentArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_ref(&args.change)?.id(&repo).await?;
    let anchor = match (&args.path, args.line) {
        (None, _) => ThreadAnchor::Change,
        (Some(path), line) => {
            let version = repo.current_version(id).await?.hash;
            let path = RepoPathBuf::new(path)?;
            match line {
                None => ThreadAnchor::File { version, path },
                Some(line) => ThreadAnchor::Lines {
                    version,
                    path,
                    start: line,
                    end: line,
                    context_hash: String::new(),
                },
            }
        }
    };
    let new = NewComment {
        anchor,
        body: args.message,
        category: None,
        confidence: None,
        to: None,
    };
    let thread = repo.reviews().comment(id, new).await?;
    ctx.emit(json::new_thread(&thread), |out| {
        out.line(format!("opened thread {}", out.id(&thread.thread_id)))
    })
}

async fn threads(ctx: &mut Ctx<'_>, args: ChangeArg) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let info = repo.reviews().get(id).await?;
    let threads = info.review.threads;
    ctx.emit(
        j!({ "change_id": id.to_string(), "threads": threads.iter().map(json::thread).collect::<Vec<_>>() }),
        |out| {
            if threads.is_empty() {
                return out.line(out.dim("no threads"));
            }
            for t in &threads {
                let state = if t.resolved {
                    out.good("resolved")
                } else {
                    out.warn("open")
                };
                let anchor = match &t.anchor {
                    ThreadAnchor::Change => String::new(),
                    ThreadAnchor::File { path, .. } => format!(" {path}"),
                    ThreadAnchor::Lines { path, start, end, .. } => format!(" {path}:{start}-{end}"),
                };
                out.line(format!("{} [{state}]{anchor}", out.id(&t.id)))?;
                for c in &t.comments {
                    out.line(format!("    {} {}: {}", out.dim(&when(&c.created_at)), c.author, c.body))?;
                }
            }
            Ok(())
        },
    )
}

async fn resolve_thread(ctx: &mut Ctx<'_>, args: ResolveThreadArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_ref(&args.change)?.id(&repo).await?;
    let thread = repo
        .reviews()
        .resolve_thread(id, &args.thread, !args.reopen)
        .await?;
    ctx.emit(json::thread(&thread), |out| {
        out.line(format!(
            "thread {} {}",
            out.id(&thread.id),
            if thread.resolved {
                "resolved"
            } else {
                "reopened"
            }
        ))
    })
}

async fn evaluate(ctx: &mut Ctx<'_>, args: ChangeArg) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let decision = repo.reviews().evaluate_submit(id).await?;
    let allowed = decision.allowed;
    ctx.emit(json::submit_decision(&decision), |out| {
        out.line(format!(
            "{}: {} (risk {})",
            out.id(&id.to_string()),
            if decision.allowed {
                out.good("landable")
            } else {
                out.bad("not landable")
            },
            decision.risk
        ))?;
        for r in &decision.reasons {
            out.line(format!("  - {r}"))?;
        }
        if !decision.matched_policies.is_empty() {
            out.line(out.dim(&format!(
                "policies: {}",
                decision.matched_policies.join(", ")
            )))?;
        }
        for r in &decision.risk_reasons {
            out.line(out.dim(&format!("risk: {r}")))?;
        }
        Ok(())
    })?;
    if allowed {
        Ok(())
    } else {
        Err(CliError::conflict("change is not landable"))
    }
}

async fn abandon(ctx: &mut Ctx<'_>, args: AbandonArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_ref(&args.change)?.id(&repo).await?;
    let state = repo.reviews().abandon(id, args.message).await?;
    ctx.emit(json::review_state(&state), |out| {
        out.line(format!("abandoned {}", out.id(&id.to_string())))
    })
}
