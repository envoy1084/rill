//! Landing and trunk commands: land, trunk, landings, verify, revert,
//! bisect.

use std::time::Duration;

use clap::Args;
use rill_mcp::json;
use rill_sdk::VerificationRecord;
use serde_json::json as j;

use crate::commands::review::change_or_working;
use crate::commands::seq_arg;
use crate::context::Ctx;
use crate::error::CliError;
use crate::output::{short, when, Out, Table};
use crate::Result;

/// `rill land`.
#[derive(Args, Debug)]
pub struct LandArgs {
    /// Change to land (default: the checkout's working change).
    pub change: Option<String>,
    /// Wait until the landing finishes.
    #[arg(long)]
    pub wait: bool,
    /// Seconds to wait with --wait.
    #[arg(long, default_value_t = 300)]
    pub timeout: u64,
}

/// `rill landings`.
#[derive(Args, Debug)]
pub struct LandingsArgs {
    /// Maximum landings.
    #[arg(long, short = 'n', default_value_t = 20)]
    pub limit: u32,
    /// Only landings before this sequence.
    #[arg(long)]
    pub before: Option<String>,
}

/// `rill verify`.
#[derive(Args, Debug)]
pub struct VerifyArgs {
    /// Trunk sequence (`7` or `trunk@7`).
    pub seq: String,
}

/// `rill revert`.
#[derive(Args, Debug)]
pub struct RevertArgs {
    /// Trunk sequence to revert.
    pub seq: String,
    /// Why.
    #[arg(short, long)]
    pub message: String,
}

/// `rill bisect`.
#[derive(Args, Debug)]
pub struct BisectArgs {
    /// Known-good sequence.
    pub from: String,
    /// Known-bad sequence.
    pub to: String,
    /// Verification target to bisect.
    #[arg(long, default_value = "default")]
    pub target: String,
}

/// Run `rill land`.
pub async fn land(ctx: &mut Ctx<'_>, args: LandArgs) -> Result<()> {
    let repo = ctx.repo()?;
    let id = change_or_working(ctx, args.change.as_deref(), &repo).await?;
    let ticket = repo.reviews().request_land(id).await?;
    if !args.wait {
        return ctx.emit(json::land_ticket(&ticket), |out| {
            out.line(format!(
                "land requested: ticket {} ({:?}); poll with `rill land --wait` or the ticket",
                out.id(&ticket.id),
                ticket.status
            ))
        });
    }
    let progress = repo
        .stack()
        .await_land(&ticket.id, Duration::from_secs(args.timeout))
        .await;
    match progress {
        Ok(progress) => ctx.emit(json::land_progress(&progress), |out| {
            out.line(format!(
                "landed {} as trunk@{} ({})",
                out.id(&id.to_string()),
                progress.seq.map(|s| s.get()).unwrap_or_default(),
                progress.landed_hash.map(|h| short(&h)).unwrap_or_default()
            ))
        }),
        Err(rill_sdk::Error::LandFailed {
            ticket,
            status,
            message,
        }) => {
            ctx.emit(
                j!({ "ticket": ticket, "status": json::enum_name(&status), "error": message }),
                |out| out.line(out.bad(&format!("landing {status:?}: {message}"))),
            )?;
            Err(CliError::conflict(format!("landing {status:?}: {message}")))
        }
        Err(e) => Err(e.into()),
    }
}

/// Run `rill trunk`.
pub async fn trunk(ctx: &mut Ctx<'_>) -> Result<()> {
    let status = ctx.repo()?.lander().trunk_status(10).await?;
    ctx.emit(json::trunk_status(&status), |out| {
        out.kv(&[
            (
                "head",
                format!(
                    "trunk@{} ({})",
                    status.head_seq.get(),
                    short(&status.head_hash)
                ),
            ),
            (
                "verified",
                status
                    .last_verified_seq
                    .map(|s| format!("trunk@{}", s.get()))
                    .unwrap_or_else(|| "none".into()),
            ),
            ("in flight", status.in_flight.len().to_string()),
        ])?;
        for l in &status.in_flight {
            out.line(format!(
                "  {} {} {:?}",
                out.id(&l.ticket.id),
                l.ticket.change_id,
                l.ticket.status
            ))?;
        }
        if !status.recent.is_empty() {
            out.blank()?;
            let mut table = Table::new(&["SEQ", "VERSION", "CHANGE", "BY", "WHEN", "REVERTS"]);
            for e in &status.recent {
                table.row(vec![
                    format!("trunk@{}", e.seq.get()),
                    out.id(&short(&e.hash)),
                    e.change_id.to_string(),
                    e.landed_by.to_string(),
                    when(&e.at),
                    e.reverts_seq
                        .map(|s| format!("trunk@{}", s.get()))
                        .unwrap_or_default(),
                ]);
            }
            out.table(table)?;
        }
        Ok(())
    })
}

fn verification_text(out: &Out<'_>, v: Option<&VerificationRecord>) -> String {
    match v {
        None => out.dim("-"),
        Some(v) => {
            let name = format!("{:?}", v.status).to_ascii_lowercase();
            match v.status {
                rill_sdk::VerificationStatus::Passed => out.good(&name),
                rill_sdk::VerificationStatus::Failed | rill_sdk::VerificationStatus::Inherited => {
                    out.bad(&name)
                }
                _ => out.warn(&name),
            }
        }
    }
}

/// Run `rill landings`.
pub async fn landings(ctx: &mut Ctx<'_>, args: LandingsArgs) -> Result<()> {
    let before = args.before.as_deref().map(seq_arg).transpose()?;
    let landings = ctx.repo()?.lander().landings(args.limit, before).await?;
    ctx.emit(
        j!({ "landings": landings.iter().map(json::landing).collect::<Vec<_>>() }),
        |out| {
            if landings.is_empty() {
                return out.line(out.dim("no landings"));
            }
            let mut table = Table::new(&["SEQ", "VERSION", "CHANGE", "BY", "WHEN", "VERIFICATION"]);
            for l in &landings {
                table.row(vec![
                    format!("trunk@{}", l.event.seq.get()),
                    out.id(&short(&l.event.hash)),
                    l.event.change_id.to_string(),
                    l.event.landed_by.to_string(),
                    when(&l.event.at),
                    verification_text(out, l.verification.as_ref()),
                ]);
            }
            out.table(table)
        },
    )
}

/// Run `rill verify`.
pub async fn verify(ctx: &mut Ctx<'_>, args: VerifyArgs) -> Result<()> {
    let seq = seq_arg(&args.seq)?;
    let record = ctx.repo()?.lander().verification(seq).await?;
    ctx.emit(json::verification_record(&record), |out| {
        out.kv(&[
            ("seq", format!("trunk@{}", record.seq.get())),
            ("status", verification_text(out, Some(&record))),
            (
                "culprit",
                record
                    .culprit
                    .map(|s| format!("trunk@{}", s.get()))
                    .unwrap_or_default(),
            ),
        ])?;
        for r in &record.reports {
            out.line(format!(
                "  {:<20} {:<10} {} {} {}",
                r.target,
                format!("{:?}", r.status).to_ascii_lowercase(),
                r.reporter,
                when(&r.reported_at),
                r.details.clone().unwrap_or_default()
            ))?;
        }
        Ok(())
    })
}

/// Run `rill revert`.
pub async fn revert(ctx: &mut Ctx<'_>, args: RevertArgs) -> Result<()> {
    let seq = seq_arg(&args.seq)?;
    let result = ctx.repo()?.lander().revert(seq, &args.message).await?;
    ctx.emit(json::revert_result(&result), |out| {
        out.line(format!(
            "reverted trunk@{} as trunk@{} ({})",
            seq.get(),
            result.landing.seq.get(),
            short(&result.landing.hash)
        ))
    })
}

/// Run `rill bisect`.
pub async fn bisect(ctx: &mut Ctx<'_>, args: BisectArgs) -> Result<()> {
    let from = seq_arg(&args.from)?;
    let to = seq_arg(&args.to)?;
    let result = ctx.repo()?.lander().bisect(from, to, &args.target).await?;
    ctx.emit(json::bisect_result(&result), |out| {
        match result.culprit {
            Some(c) => out.line(format!(
                "culprit: {}",
                out.bad(&format!("trunk@{}", c.get()))
            ))?,
            None => out.line("no culprit found")?,
        }
        for t in &result.tested {
            out.line(format!(
                "  trunk@{} {}",
                t.seq.get(),
                verification_text(out, Some(t))
            ))?;
        }
        Ok(())
    })
}
