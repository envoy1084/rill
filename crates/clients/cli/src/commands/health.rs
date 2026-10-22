//! `rill trunk health|incidents|freeze|thaw`: whether trunk is red, why,
//! and stopping landings while it is fixed.

use bytes::Bytes;
use clap::{Args, Subcommand};
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;
use serde_json::{json as j, Value};

use crate::context::Ctx;
use crate::output::{when, Table};
use crate::Result;

/// `rill trunk [subcommand]`.
#[derive(Args, Debug)]
pub struct TrunkArgs {
    /// What to do; status when absent.
    #[command(subcommand)]
    pub cmd: Option<TrunkCmd>,
}

/// What `rill trunk` can do besides showing status.
#[derive(Subcommand, Debug)]
pub enum TrunkCmd {
    /// Trunk's state (green, red, fixing, guarded) and open incidents.
    Health,
    /// Incidents, open and closed, newest first.
    Incidents(IncidentsArgs),
    /// Let only emergency landings land (repository owners).
    Freeze(FreezeArgs),
    /// Lift a freeze (repository owners).
    Thaw,
}

/// `rill trunk incidents`.
#[derive(Args, Debug)]
pub struct IncidentsArgs {
    /// Most incidents to show.
    #[arg(long, short = 'n', default_value_t = 20)]
    pub limit: u32,
}

/// `rill trunk freeze`.
#[derive(Args, Debug)]
pub struct FreezeArgs {
    /// Why, shown with the health.
    #[arg(long, default_value = "")]
    pub reason: String,
}

/// Run a trunk subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: TrunkCmd) -> Result<()> {
    let repo = ctx.repo_name()?;
    let mut svc = ctx.client()?.lander_svc();
    match cmd {
        TrunkCmd::Health => {
            let reply = svc
                .get_trunk_health(pb::GetTrunkHealthRequest { repo })
                .await?
                .into_inner();
            let health = reply.health.unwrap_or_default();
            ctx.emit(
                j!({ "health": health_json(&health), "open": reply.open.iter().map(incident_json).collect::<Vec<_>>() }),
                |out| {
                    let mut state = health.state.clone();
                    if health.frozen {
                        state.push_str(", frozen");
                        if let Some(reason) = &health.frozen_reason {
                            state.push_str(&format!(" ({reason})"));
                        }
                    }
                    out.kv(&[
                        ("state", state),
                        ("since", when(&Timestamp::from_millis(health.since))),
                        ("failing", health.failing_steps.join(", ")),
                    ])?;
                    incidents_table(out, &reply.open)
                },
            )
        }
        TrunkCmd::Incidents(args) => {
            let incidents = svc
                .list_incidents(pb::ListIncidentsRequest {
                    repo,
                    limit: args.limit,
                })
                .await?
                .into_inner()
                .incidents;
            ctx.emit(
                j!({ "incidents": incidents.iter().map(incident_json).collect::<Vec<_>>() }),
                |out| incidents_table(out, &incidents),
            )
        }
        TrunkCmd::Freeze(args) => freeze(ctx, &mut svc, repo, true, args.reason).await,
        TrunkCmd::Thaw => freeze(ctx, &mut svc, repo, false, String::new()).await,
    }
}

async fn freeze(
    ctx: &mut Ctx<'_>,
    svc: &mut rill_sdk::client::LanderSvc,
    repo: String,
    frozen: bool,
    reason: String,
) -> Result<()> {
    let health = svc
        .freeze_trunk(pb::FreezeTrunkRequest {
            operation_id: Bytes::new(),
            repo,
            frozen,
            reason,
        })
        .await?
        .into_inner()
        .health
        .unwrap_or_default();
    ctx.emit(j!({ "health": health_json(&health) }), |out| {
        out.line(match frozen {
            true => "trunk frozen: only emergency landings land".to_string(),
            false => "trunk thawed".to_string(),
        })
    })
}

fn incidents_table(
    out: &mut crate::output::Out<'_>,
    incidents: &[pb::Incident],
) -> std::io::Result<()> {
    if incidents.is_empty() {
        return out.line(out.dim("no incidents"));
    }
    let mut table = Table::new(&[
        "CULPRIT",
        "STATE",
        "STEPS",
        "AUTHOR",
        "OPENED",
        "RESOLUTION",
    ]);
    for i in incidents {
        table.row(vec![
            format!("trunk@{}", i.culprit),
            i.state.clone(),
            i.steps.join(", "),
            i.author
                .as_ref()
                .map(|p| p.name.clone())
                .unwrap_or_default(),
            when(&Timestamp::from_millis(i.opened_at)),
            match (&i.resolution, i.resolved_at) {
                (Some(how), Some(at)) => format!("{how} at trunk@{at}"),
                _ => i
                    .revert_blocked
                    .as_ref()
                    .map(|r| format!("revert blocked: {r}"))
                    .unwrap_or_default(),
            },
        ]);
    }
    out.table(table)
}

fn health_json(h: &pb::TrunkHealth) -> Value {
    j!({
        "state": h.state,
        "since": h.since,
        "open": h.open,
        "failing_steps": h.failing_steps,
        "frozen": h.frozen,
        "frozen_reason": h.frozen_reason,
    })
}

fn incident_json(i: &pb::Incident) -> Value {
    j!({
        "culprit": i.culprit,
        "state": i.state,
        "steps": i.steps,
        "failures": i.failures,
        "author": i.author.as_ref().map(|p| p.name.clone()),
        "session": i.session,
        "model": i.model,
        "opened_at": i.opened_at,
        "fix": i.fix,
        "resolution": i.resolution,
        "resolved_at": i.resolved_at,
        "revert_blocked": i.revert_blocked,
        "assigned": i.assigned,
    })
}

/// `rill revert --session …`: preview or queue reverts of every landing a
/// selector matches.
pub async fn revert_set(ctx: &mut Ctx<'_>, args: crate::commands::land::RevertArgs) -> Result<()> {
    let repo = ctx.repo_name()?;
    let seq = |s: &Option<String>| -> Result<Option<u64>> {
        s.as_deref()
            .map(|s| crate::commands::seq_arg(s).map(|q| q.get()))
            .transpose()
    };
    let intent = args
        .intent
        .as_deref()
        .map(|i| {
            i.parse::<rill_types::IntentId>()
                .map(|id| rill_proto::convert::id_to_proto(&id))
                .map_err(|e| crate::error::CliError::usage(format!("--intent: {e}")))
        })
        .transpose()?;
    let request = pb::RevertSetRequest {
        operation_id: Bytes::new(),
        repo,
        session: args.session,
        model: args.model,
        sponsor: args.sponsor,
        author: args.author,
        intent,
        since: seq(&args.since)?,
        until: seq(&args.until)?,
        apply: args.apply,
        reason: args.message,
    };
    let entries = ctx
        .client()?
        .lander_svc()
        .revert_set(request)
        .await?
        .into_inner()
        .entries;
    let apply = args.apply;
    ctx.emit(
        j!({ "applied": apply, "entries": entries.iter().map(|e| j!({
            "seq": e.seq,
            "summary": e.summary,
            "conflicts": e.conflicts,
            "ticket": e.ticket,
        })).collect::<Vec<_>>() }),
        |out| {
            if entries.is_empty() {
                return out.line(out.dim("nothing matches"));
            }
            if apply {
                for e in &entries {
                    out.line(format!(
                        "revert of trunk@{} queued: ticket {}",
                        e.seq,
                        e.ticket.clone().unwrap_or_default()
                    ))?;
                }
                return Ok(());
            }
            let mut table = Table::new(&["SEQ", "SUMMARY", "CONFLICTS"]);
            for e in &entries {
                table.row(vec![
                    format!("trunk@{}", e.seq),
                    e.summary.clone(),
                    e.conflicts.join(", "),
                ]);
            }
            out.table(table)?;
            out.line(out.dim("a preview: run again with --apply to queue these reverts"))
        },
    )
}
