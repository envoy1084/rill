//! `rill checks quarantine`: targets whose failures neither block
//! landings nor make trunk red.

use bytes::Bytes;
use clap::{Args, Subcommand};
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;
use serde_json::json as j;

use crate::context::Ctx;
use crate::output::{when, Table};
use crate::Result;

/// `rill checks quarantine <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum QuarantineCmd {
    /// Quarantined targets.
    List,
    /// Quarantine a step (`step:<name>`) or test (`test:<id>`) (owners).
    Add(AddArgs),
    /// Lift a quarantine (owners).
    Release(TargetArg),
}

/// `rill checks quarantine add`.
#[derive(Args, Debug)]
pub struct AddArgs {
    pub target: String,
    /// Why, recorded with it.
    #[arg(long, default_value = "")]
    pub reason: String,
}

/// A command naming one target.
#[derive(Args, Debug)]
pub struct TargetArg {
    pub target: String,
}

pub async fn run(ctx: &mut Ctx<'_>, cmd: QuarantineCmd) -> Result<()> {
    let repo = ctx.repo_name()?;
    let mut svc = ctx.client()?.checks_svc();
    match cmd {
        QuarantineCmd::List => {
            let targets = svc
                .list_quarantined(pb::ListQuarantinedRequest { repo })
                .await?
                .into_inner()
                .targets;
            ctx.emit(
                j!({ "targets": targets.iter().map(|t| j!({
                    "target": t.target,
                    "since": t.since,
                    "by": t.by.as_ref().map(|p| p.name.clone()),
                    "reason": t.reason,
                    "intent": t.intent.as_ref().map(hex::encode),
                })).collect::<Vec<_>>() }),
                |out| {
                    let mut table = Table::new(&["TARGET", "SINCE", "BY", "REASON"]);
                    for t in &targets {
                        table.row(vec![
                            t.target.clone(),
                            when(&Timestamp::from_millis(t.since)),
                            t.by.as_ref().map(|p| p.name.clone()).unwrap_or_default(),
                            t.reason.clone(),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        QuarantineCmd::Add(args) => {
            let added = svc
                .quarantine(pb::QuarantineRequest {
                    operation_id: Bytes::new(),
                    repo,
                    target: args.target.clone(),
                    reason: args.reason,
                })
                .await?
                .into_inner()
                .added;
            ctx.emit(j!({ "target": args.target, "added": added }), |out| {
                out.line(match added {
                    true => format!("quarantined {}", args.target),
                    false => format!("{} was already quarantined", args.target),
                })
            })
        }
        QuarantineCmd::Release(args) => {
            let released = svc
                .release_quarantine(pb::ReleaseQuarantineRequest {
                    operation_id: Bytes::new(),
                    repo,
                    target: args.target.clone(),
                })
                .await?
                .into_inner()
                .released;
            ctx.emit(j!({ "target": args.target, "released": released }), |out| {
                out.line(match released {
                    true => format!("lifted the quarantine of {}", args.target),
                    false => format!("{} was not quarantined", args.target),
                })
            })
        }
    }
}
