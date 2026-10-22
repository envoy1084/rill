//! `rill checks flaky` and `rill checks history`.

use clap::Args;
use rill_proto::rill::v1 as pb;
use rill_types::Timestamp;
use serde_json::json as j;

use crate::context::Ctx;
use crate::output::{when, Table};
use crate::Result;

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

pub(super) fn outcome(value: i32) -> &'static str {
    match rill_proto::convert::known::<pb::TestOutcome>(value)
        .unwrap_or(pb::TestOutcome::Unspecified)
    {
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
        "last": outcome(t.last.to_i32()),
    })
}

/// `rill checks flaky`.
pub async fn flaky(ctx: &mut Ctx<'_>, a: FlakyArgs) -> Result<()> {
    let repo = ctx.repo_name()?;
    let svc = ctx.client()?.checks_svc();
    let tests = svc
        .list_flaky_tests(pb::ListFlakyTestsRequest {
            parent: rill_sdk::names::repo(&repo),
            page_size: a.limit as i32,
            page_token: String::new(),
            min_score: Some(a.min_score),
        })
        .await?
        .into_owned()
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
                    outcome(t.last.to_i32()).into(),
                    t.target.clone(),
                ]);
            }
            out.table(table)
        },
    )
}

/// `rill checks history`.
pub async fn history(ctx: &mut Ctx<'_>, a: HistoryArgs) -> Result<()> {
    let repo = ctx.repo_name()?;
    let svc = ctx.client()?.checks_svc();
    let response = svc
        .get_test_history(pb::GetTestHistoryRequest {
            parent: rill_sdk::names::repo(&repo),
            target: a.target,
            max_results: a.limit as i32,
        })
        .await?
        .into_owned();
    let runs = response.runs;
    ctx.emit(
        j!({
            "stats": response.stats.as_option().map(flaky_json),
            "runs": runs.iter().map(|r| j!({
                "outcome": outcome(r.outcome.to_i32()),
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
            if let Some(stats) = response.stats.as_option() {
                out.line(format!("flake score {:.2}", stats.score))?;
            }
            let mut table = Table::new(&["WHEN", "OUTCOME", "MS", "STAGE", "ACTION", "WORKER"]);
            for r in &runs {
                table.row(vec![
                    when(&Timestamp::from_millis(r.at)),
                    format!(
                        "{}{}",
                        outcome(r.outcome.to_i32()),
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

/// `rill checks nonhermetic`.
pub async fn nonhermetic(ctx: &mut Ctx<'_>) -> Result<()> {
    let repo = ctx.repo_name()?;
    let steps = ctx
        .client()?
        .checks_svc()
        .list_nonhermetic_steps(pb::ListNonhermeticStepsRequest {
            parent: rill_sdk::names::repo(&repo),
            page_size: 1000,
            page_token: String::new(),
        })
        .await?
        .into_owned()
        .steps;
    ctx.emit(
        j!({ "steps": steps.iter().map(|s| j!({
            "step": s.step,
            "action": hex::encode(&s.action),
            "differences": s.differences,
            "found_at": s.found_at,
            "until": s.until,
        })).collect::<Vec<_>>() }),
        |out| {
            let mut table = Table::new(&["STEP", "FOUND", "UNCACHED UNTIL", "DIFFERENCES"]);
            for s in &steps {
                table.row(vec![
                    s.step.clone(),
                    when(&Timestamp::from_millis(s.found_at)),
                    when(&Timestamp::from_millis(s.until)),
                    s.differences.join("; "),
                ]);
            }
            out.table(table)
        },
    )
}
