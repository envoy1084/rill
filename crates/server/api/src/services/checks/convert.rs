//! Test history and runs as protobuf messages.

use bytes::Bytes;
use rill_exec::history::{FlakeStats, TestOutcome, TestRun};
use rill_exec::runs::{IndexedStep, RunHeader, StepEnd, StepState};
use rill_proto::convert::timestamp_to_proto;
use rill_proto::rill::v1 as pb;

pub(super) fn outcome(outcome: TestOutcome) -> i32 {
    (match outcome {
        TestOutcome::Passed => pb::TestOutcome::Passed,
        TestOutcome::Failed => pb::TestOutcome::Failed,
        TestOutcome::Skipped => pb::TestOutcome::Skipped,
        TestOutcome::Error => pb::TestOutcome::Error,
    }) as i32
}

pub(super) fn test_run(run: TestRun) -> pb::TestRun {
    pb::TestRun {
        target: run.target,
        outcome: outcome(run.outcome).into(),
        retried: run.retried,
        duration_ms: run.duration_ms,
        action: run
            .action
            .map(|a| Bytes::copy_from_slice(a.as_bytes()))
            .unwrap_or_default(),
        worker: run.worker.unwrap_or_default(),
        version: Bytes::copy_from_slice(run.version.as_bytes()),
        trunk: run.trunk,
        at: timestamp_to_proto(run.at),
    }
}

pub(super) fn flaky(target: String, score: f64, stats: FlakeStats) -> pb::FlakyTest {
    pb::FlakyTest {
        target,
        score,
        runs: stats.runs,
        failures: stats.failures,
        flips: stats.flips,
        last: stats.last.map_or(0, outcome).into(),
    }
}

/// A run, with `steps` when given.
pub(super) fn run(header: &RunHeader, steps: &[IndexedStep]) -> pb::VerifyRun {
    pb::VerifyRun {
        id: header.id.clone(),
        version: Bytes::copy_from_slice(header.version.as_bytes()),
        trunk: header.trunk,
        started_at: timestamp_to_proto(header.started_at),
        green: header.end.as_ref().map(|e| e.green),
        summary: header
            .end
            .as_ref()
            .map(|e| e.summary.clone())
            .unwrap_or_default(),
        finished_at: header
            .end
            .as_ref()
            .map(|e| timestamp_to_proto(e.finished_at)),
        steps: steps.iter().map(step).collect(),
    }
}

fn step(indexed: &IndexedStep) -> pb::RunStep {
    let s = &indexed.step;
    let (state, exit_code, error) = match &s.state {
        StepState::Queued => (pb::StepState::Queued, None, String::new()),
        StepState::Running => (pb::StepState::Running, None, String::new()),
        StepState::Done(end) => match end {
            StepEnd::Passed => (pb::StepState::Passed, Some(0), String::new()),
            StepEnd::Failed { exit_code } => {
                (pb::StepState::Failed, Some(*exit_code), String::new())
            }
            StepEnd::TimedOut => (pb::StepState::TimedOut, None, String::new()),
            StepEnd::Error(message) => (pb::StepState::Error, None, message.clone()),
            StepEnd::Unaffected => (pb::StepState::Unaffected, None, String::new()),
            StepEnd::NotGating => (pb::StepState::NotGating, None, String::new()),
        },
    };
    let (shard, shards) = s.shard.map_or((0, 0), |(i, n)| (i + 1, n));
    pb::RunStep {
        index: indexed.index,
        name: s.step.clone(),
        shard,
        shards,
        state: state.into(),
        exit_code,
        error,
        cached: s.cached,
        stdout_size: s.stdout.map_or(0, |d| d.size),
        stderr_size: s.stderr.map_or(0, |d| d.size),
        artifacts: s
            .outputs
            .iter()
            .map(|(path, d)| pb::Artifact {
                path: path.clone(),
                size: d.size,
            })
            .collect(),
        worker: s.worker.clone().unwrap_or_default(),
        duration_ms: s.duration_ms,
        nonhermetic: s.nonhermetic,
    }
}
