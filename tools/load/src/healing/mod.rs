//! Self-healing trunk gate: break trunk on purpose, in the ways it breaks
//! in practice, and check the server finds, blames and repairs each break
//! within the SLOs.

mod scenarios;
mod slo;
mod trunk;

use std::time::Instant;

use serde_json::{json, Value};

pub use slo::Slo;
pub use trunk::Trunk;

/// Run every scenario in turn on `trunk` (or only the one named `only`),
/// landing background changes throughout; each scenario's report, with
/// whether all passed.
pub async fn run_scenarios(trunk: &Trunk, slo: &Slo, only: Option<&str>) -> Value {
    let background = trunk.trickle(std::time::Duration::from_secs(2));
    let wants = |name: &str| {
        let wanted = only.is_none_or(|only| only == name);
        if wanted {
            eprintln!("scenario: {name}");
        }
        wanted
    };
    let mut reports = Vec::new();
    if wants("breaking change") {
        let started = Instant::now();
        let outcome = scenarios::breaking_change(trunk, slo).await;
        reports.push(report("breaking change", started, outcome));
    }
    if wants("overlapping culprits") {
        let started = Instant::now();
        let outcome = scenarios::overlapping_culprits(trunk, slo).await;
        reports.push(report("overlapping culprits", started, outcome));
    }
    if wants("flaky test") {
        let started = Instant::now();
        let outcome = scenarios::flaky_test(trunk, slo).await;
        reports.push(report("flaky test", started, outcome));
    }
    if wants("revert conflict") {
        let started = Instant::now();
        let outcome = scenarios::revert_conflict(trunk, slo).await;
        reports.push(report("revert conflict", started, outcome));
    }
    if wants("forward fix") {
        let started = Instant::now();
        let outcome = scenarios::forward_fix(trunk, slo).await;
        reports.push(report("forward fix", started, outcome));
    }
    background.abort();
    let passed = !reports.is_empty() && reports.iter().all(|r| r["passed"] == true);
    json!({ "passed": passed, "scenarios": reports })
}

/// A scenario's report: passed when it ran through and every culprit was
/// handled within the SLO.
fn report(name: &str, started: Instant, outcome: anyhow::Result<Value>) -> Value {
    let report = match outcome {
        Ok(report) => {
            let within = report["culprits"]
                .as_array()
                .into_iter()
                .flatten()
                .all(|c| c["within_slo"] == true);
            json!({ "scenario": name, "passed": within, "report": report })
        }
        Err(e) => json!({ "scenario": name, "passed": false, "error": format!("{e:#}") }),
    };
    let mut report = report;
    report["elapsed_secs"] = json!(started.elapsed().as_secs());
    eprintln!("{report}");
    report
}
