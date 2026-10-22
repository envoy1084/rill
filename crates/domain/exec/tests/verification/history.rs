//! Test history: runs recorded from verification, flips on unchanged
//! inputs scored as flakes, and JUnit reports read from steps.

use rill_exec::history::{TestHistory, TestOutcome, TestRun, RUNS_KEPT};
use rill_store::Stores;
use rill_types::{ObjectHash, ObjectKind, Timestamp};

use crate::fixture::Fixture;

fn run(target: &str, outcome: TestOutcome, action: u8, at: i64) -> TestRun {
    TestRun {
        target: target.into(),
        outcome,
        retried: false,
        duration_ms: Some(5),
        action: Some(ObjectHash::digest(ObjectKind::Blob, &[action])),
        worker: None,
        version: ObjectHash::ZERO,
        trunk: false,
        at: Timestamp::from_millis(at),
    }
}

async fn history() -> TestHistory {
    let stores = Stores::memory();
    let repo = stores.repo(stores.create_repo("app", "").await.unwrap().id);
    TestHistory::new(repo)
}

#[tokio::test]
async fn a_verdict_that_changes_on_the_same_action_is_a_flake_and_new_inputs_are_not() {
    let history = history().await;
    let now = Timestamp::now().millis();
    history
        .record(&run("test:racy", TestOutcome::Failed, 1, now - 3))
        .await
        .unwrap();
    history
        .record(&run("test:racy", TestOutcome::Passed, 1, now - 2))
        .await
        .unwrap();
    history
        .record(&run("test:fixed", TestOutcome::Failed, 1, now - 3))
        .await
        .unwrap();
    history
        .record(&run("test:fixed", TestOutcome::Passed, 2, now - 2))
        .await
        .unwrap();

    let flaky = history.flaky(0.1, 10).await.unwrap();
    assert_eq!(flaky.len(), 1, "{flaky:?}");
    assert_eq!(flaky[0].target, "test:racy");
    assert!((flaky[0].stats.flips - 1.0).abs() < 1e-6);

    let runs = history.runs("test:racy", 10).await.unwrap();
    assert_eq!(
        runs.iter().map(|r| r.outcome).collect::<Vec<_>>(),
        [TestOutcome::Passed, TestOutcome::Failed]
    );
}

#[tokio::test]
async fn only_the_latest_runs_are_kept() {
    let history = history().await;
    for i in 0..(RUNS_KEPT as i64 + 5) {
        history
            .record(&run("step:unit", TestOutcome::Passed, 1, 1_000 + i))
            .await
            .unwrap();
    }
    let runs = history.runs("step:unit", 1_000).await.unwrap();
    assert_eq!(runs.len(), RUNS_KEPT);
    assert_eq!(runs[0].at.millis(), 1_000 + RUNS_KEPT as i64 + 4);
}

#[tokio::test]
async fn a_steps_junit_report_becomes_its_test_cases() {
    let fx = Fixture::new().await;
    let plan = r#"
[[step]]
name = "unit"
command = ["sh", "-c", '''mkdir -p out && cat > out/junit.xml <<XML
<testsuites><testsuite name="s" tests="2"><testcase classname="core" name="adds" time="0.5"/><testcase classname="core" name="subtracts"><failure message="off by one"/></testcase></testsuite></testsuites>
XML
''']
junit = "out/junit.xml"
"#;
    let outcome = fx
        .verify(fx.tree(None, &[("PLAN", plan)]).await, None)
        .await;
    assert!(outcome.green, "{:?}", outcome);
    let tests = &outcome.steps[0].tests;
    assert_eq!(tests.len(), 2);
    assert_eq!(
        (tests[0].id.as_str(), tests[0].outcome),
        ("core::adds", TestOutcome::Passed)
    );
    assert_eq!(tests[1].outcome, TestOutcome::Failed);

    let runs = outcome.test_runs(ObjectHash::ZERO, false, Timestamp::now());
    let targets: Vec<_> = runs.iter().map(|r| r.target.as_str()).collect();
    assert_eq!(
        targets,
        ["step:unit", "test:core::adds", "test:core::subtracts"]
    );
    assert!(runs
        .iter()
        .all(|r| r.action == outcome.steps[0].action.map(|a| a.hash)));
}
