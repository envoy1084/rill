//! `rill checks quarantine`: targets whose failures neither block
//! landings nor make trunk red.

use clap::{Args, Subcommand};
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
    /// `step:<name>` or `test:<id>`.
    pub target: String,
    /// Why, recorded with it.
    #[arg(long, default_value = "")]
    pub reason: String,
}

/// A command naming one target.
#[derive(Args, Debug)]
pub struct TargetArg {
    /// `step:<name>` or `test:<id>`.
    pub target: String,
}

/// Run a quarantine subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: QuarantineCmd) -> Result<()> {
    let checks = ctx.repo()?.checks();
    match cmd {
        QuarantineCmd::List => {
            let targets = checks.quarantines().await?;
            ctx.emit(
                j!({ "targets": targets.iter().map(|t| j!({
                    "target": t.target,
                    "since": t.since,
                    "by": t.by.as_option().map(|p| p.name.clone()),
                    "reason": t.reason,
                    "intent": t.intent.as_ref().map(hex::encode),
                })).collect::<Vec<_>>() }),
                |out| {
                    let mut table = Table::new(&["TARGET", "SINCE", "BY", "REASON"]);
                    for t in &targets {
                        table.row(vec![
                            t.target.clone(),
                            when(&Timestamp::from_millis(t.since)),
                            t.by.as_option().map(|p| p.name.clone()).unwrap_or_default(),
                            t.reason.clone(),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        QuarantineCmd::Add(args) => {
            checks.quarantine(&args.target, &args.reason).await?;
            ctx.emit(j!({ "target": args.target, "added": true }), |out| {
                out.line(format!("quarantined {}", args.target))
            })
        }
        QuarantineCmd::Release(args) => {
            checks.release_quarantine(&args.target).await?;
            ctx.emit(j!({ "target": args.target, "released": true }), |out| {
                out.line(format!("lifted the quarantine of {}", args.target))
            })
        }
    }
}
