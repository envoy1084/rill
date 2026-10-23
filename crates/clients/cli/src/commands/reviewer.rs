//! `rill reviewer`: reviewers' track records, and reinstating a demoted
//! one.

use clap::{Args, Subcommand};
use rill_mcp::json;

use crate::context::Ctx;
use crate::output::Table;
use crate::Result;

/// `rill reviewer <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ReviewerCmd {
    /// Each reviewer's outcomes and trust in this repository.
    Stats,
    /// Clear a demoted reviewer's record so it reviews again.
    Reinstate(ReinstateArgs),
}

/// `rill reviewer reinstate`.
#[derive(Args, Debug)]
pub struct ReinstateArgs {
    /// The reviewer agent.
    pub reviewer: String,
}

/// Run a reviewer subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: ReviewerCmd) -> Result<()> {
    let reviewers = ctx.repo()?.reviewers();
    match cmd {
        ReviewerCmd::Stats => {
            let stats = reviewers.stats().await?;
            ctx.emit(
                serde_json::json!(stats.iter().map(json::reviewer_stats).collect::<Vec<_>>()),
                |out| {
                    if stats.is_empty() {
                        return out.line(out.dim("no reviewer outcomes yet"));
                    }
                    let mut table = Table::new(&[
                        "REVIEWER",
                        "TRUST",
                        "OUTCOMES",
                        "APPROVED",
                        "ESCAPED",
                        "CAUGHT",
                        "DISMISSED",
                        "STATE",
                    ]);
                    for s in &stats {
                        table.row(vec![
                            out.bold(&s.reviewer),
                            format!("{:.2}", f64::from(s.trust) / 1000.0),
                            s.outcomes.to_string(),
                            s.approvals.to_string(),
                            s.escapes.to_string(),
                            s.blocks.to_string(),
                            s.dismissed.to_string(),
                            if s.demoted {
                                out.warn("demoted")
                            } else {
                                out.good("active")
                            },
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        ReviewerCmd::Reinstate(a) => {
            let s = reviewers.reinstate(&a.reviewer).await?;
            ctx.emit(json::reviewer_stats(&s), |out| {
                out.line(format!("{} reinstated with a clean record", s.reviewer))
            })
        }
    }
}
