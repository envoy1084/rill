//! `rill checks`: what verification ran, test by test.

use clap::{Args, Subcommand};
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;
use serde_json::json as j;

use crate::context::Ctx;
use crate::output::{when, Table};
use crate::Result;

/// `rill checks <subcommand>`.
#[derive(Subcommand, Debug)]
pub enum ChecksCmd {
    /// Steps and tests whose outcome changes on unchanged inputs.
    Flaky(FlakyArgs),
    /// Recent runs of one step (`step:<name>`) or test (`test:<id>`).
    History(HistoryArgs),
}

/// `rill checks flaky`.
#[derive(Args, Debug)]
pub struct FlakyArgs {
    /// Only targets scoring at least this (0 to 1).
    #[arg(long, default_value_t = 0.1)]
    pub min_score: f64,
    /// At most this many.
    #[arg(long, default_value_t = 50)]
    pub limit: u32,
}

/// `rill checks history`.
#[derive(Args, Debug)]
pub struct HistoryArgs {
    /// The target, as `rill checks flaky` lists it.
    pub target: String,
    /// At most this many runs.
    #[arg(long, default_value_t = 20)]
    pub limit: u32,
}

fn outcome(value: i32) -> &'static str {
    match pb::TestOutcome::try_from(value).unwrap_or(pb::TestOutcome::Unspecified) {
        pb::TestOutcome::Passed => "passed",
        pb::TestOutcome::Failed => "failed",
        pb::TestOutcome::Skipped => "skipped",
        pb::TestOutcome::Error => "error",
        pb::TestOutcome::Unspecified => "-",
    }
}

fn flaky_json(t: &pb::FlakyTest) -> serde_json::Value {
    j!({
        "target": t.target,
        "score": t.score,
        "runs": t.runs,
        "failures": t.failures,
        "flips": t.flips,
        "last": outcome(t.last),
    })
}

/// Run a checks subcommand.
pub async fn run(ctx: &mut Ctx<'_>, cmd: ChecksCmd) -> Result<()> {
    let repo = ctx.repo_name()?;
    let mut svc = ctx.client()?.checks_svc();
    match cmd {
        ChecksCmd::Flaky(a) => {
            let tests = svc
                .list_flaky_tests(pb::ListFlakyTestsRequest {
                    repo,
                    min_score: Some(a.min_score),
                    limit: a.limit,
                })
                .await?
                .into_inner()
                .tests;
            ctx.emit(
                j!({ "tests": tests.iter().map(flaky_json).collect::<Vec<_>>() }),
                |out| {
                    let mut table = Table::new(&["SCORE", "FLIPS", "RUNS", "LAST", "TARGET"]);
                    for t in &tests {
                        table.row(vec![
                            format!("{:.2}", t.score),
                            format!("{:.1}", t.flips),
                            format!("{:.1}", t.runs),
                            outcome(t.last).into(),
                            t.target.clone(),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
        ChecksCmd::History(a) => {
            let response = svc
                .get_test_history(pb::GetTestHistoryRequest {
                    repo,
                    target: a.target,
                    limit: a.limit,
                })
                .await?
                .into_inner();
            let runs = response.runs;
            ctx.emit(
                j!({
                    "stats": response.stats.as_ref().map(flaky_json),
                    "runs": runs.iter().map(|r| j!({
                        "outcome": outcome(r.outcome),
                        "retried": r.retried,
                        "duration_ms": r.duration_ms,
                        "action": hex::encode(&r.action),
                        "worker": r.worker,
                        "version": hex::encode(&r.version),
                        "trunk": r.trunk,
                        "at": r.at,
                    })).collect::<Vec<_>>(),
                }),
                |out| {
                    if let Some(stats) = &response.stats {
                        out.line(format!("flake score {:.2}", stats.score))?;
                    }
                    let mut table =
                        Table::new(&["WHEN", "OUTCOME", "MS", "STAGE", "ACTION", "WORKER"]);
                    for r in &runs {
                        table.row(vec![
                            when(&Timestamp::from_millis(r.at)),
                            format!(
                                "{}{}",
                                outcome(r.outcome),
                                if r.retried { " (retried)" } else { "" }
                            ),
                            r.duration_ms.map(|d| d.to_string()).unwrap_or_default(),
                            if r.trunk { "trunk" } else { "pre-land" }.into(),
                            hex::encode(&r.action).chars().take(12).collect(),
                            r.worker.clone(),
                        ]);
                    }
                    out.table(table)
                },
            )
        }
    }
}
