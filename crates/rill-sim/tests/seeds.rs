//! Seeded simulation runs as ordinary tests. `RILL_SIM_SEEDS` raises the
//! number of seeds for longer local runs.

use rill_sim::{known, run, Report, Scenario};

fn seeds(default: u64) -> std::ops::Range<u64> {
    let n = std::env::var("RILL_SIM_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default);
    0..n
}

fn assert_clean(report: &Report) {
    assert!(
        report.violations.is_empty(),
        "{} seed {} broke invariants (replay: cargo run -p rill-sim -- --scenario {} --seed {}):\n{}",
        report.scenario,
        report.seed,
        report.scenario,
        report.seed,
        report
            .violations
            .iter()
            .map(|v| format!("  {v}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn fault_free_runs_keep_every_invariant() {
    let scenario = Scenario::clean();
    for seed in seeds(1000) {
        assert_clean(&run(seed, &scenario));
    }
}

#[test]
fn contended_runs_keep_every_invariant() {
    let scenario = Scenario::contended();
    for seed in seeds(300) {
        assert_clean(&run(seed, &scenario));
    }
}

#[test]
fn faulty_runs_break_only_known_invariants() {
    let scenario = Scenario::faulty();
    let (mut faults, mut killed, mut fenced, mut cut, mut rival) = (0, 0, 0, 0, 0);
    for seed in seeds(1000) {
        let report = run(seed, &scenario);
        faults += report.faults_injected;
        killed += report.killed_jobs;
        fenced += report.fenced_attempts;
        cut += report.partitions;
        rival += report.rival_landings;
        for v in &report.violations {
            assert!(
                known(v.invariant).is_some(),
                "seed {seed}: unknown violation {v} (replay: cargo run -p rill-sim -- --seed {seed})"
            );
        }
    }
    assert!(faults > 0, "the faulty scenario injected no faults");
    assert!(killed > 0, "no worker died holding a job");
    assert!(fenced > 0, "no replaced lander tried to land");
    assert!(cut > 0, "no process was partitioned");
    assert!(rival > 0, "the rival process never landed");
}

#[test]
fn a_seed_replays_identically() {
    let scenario = Scenario::faulty();
    for seed in [3, 41, 977] {
        let (a, b) = (run(seed, &scenario), run(seed, &scenario));
        assert_eq!(a.digest, b.digest, "seed {seed} diverged");
        assert_eq!(a.violations, b.violations);
    }
}
