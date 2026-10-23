//! The review-at-scale gate: 1,000 people and 30,000 agents, at most a
//! tenth of changes to people and nine in ten seeded defects caught.
//! `RILL_GATE_CHANGES` raises the number of changes for longer runs.

use rill_sim::review_gate::{run_gate, GateConfig};

#[tokio::test(flavor = "multi_thread")]
async fn review_at_scale_meets_the_gate() {
    let changes = std::env::var("RILL_GATE_CHANGES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_500);
    let report = run_gate(GateConfig {
        changes,
        ..GateConfig::default()
    })
    .await;
    println!("{report:?}");
    println!(
        "to humans: {}‰, defects caught: {}‰",
        report.human_permille(),
        report.caught_permille()
    );
    assert!(report.passes(), "{report:?}");
}
