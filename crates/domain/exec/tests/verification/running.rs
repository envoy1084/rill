//! Steps run on the worker, with their inputs, and come back from the
//! cache when nothing they read changed.

use rill_exec::verify::StepStatus;

use crate::fixture::Fixture;

const CHECK_MAIN: &str = r#"
[[step]]
name = "check"
command = ["sh", "-c", "test -f src/main.rs"]
inputs = ["src/**"]
"#;

#[tokio::test]
async fn a_passing_plan_runs_once_then_comes_from_the_cache() {
    let fx = Fixture::new().await;
    let root = fx
        .tree(
            None,
            &[("PLAN", CHECK_MAIN), ("src/main.rs", "fn main() {}")],
        )
        .await;

    let first = fx.verify(root, None).await;
    assert!(first.green, "{}", first.summary());
    assert_eq!(first.steps[0].status, StepStatus::Passed);
    assert!(!first.steps[0].cached);
    assert_eq!(first.steps[0].worker.as_deref(), Some("test-worker"));

    let second = fx.verify(root, None).await;
    assert!(second.green && second.steps[0].cached);
    assert_eq!(second.steps[0].action, first.steps[0].action);
}

#[tokio::test]
async fn a_failing_step_is_red_with_the_end_of_its_output() {
    let fx = Fixture::new().await;
    let plan =
        "[[step]]\nname = \"boom\"\ncommand = [\"sh\", \"-c\", \"echo went wrong >&2; exit 3\"]\n";
    let outcome = fx
        .verify(fx.tree(None, &[("PLAN", plan)]).await, None)
        .await;
    assert!(!outcome.green);
    assert_eq!(
        outcome.first_failure().unwrap().status,
        StepStatus::Failed { exit_code: 3 }
    );
    assert!(outcome
        .summary()
        .contains("\"boom\" failed with exit code 3"));
    assert_eq!(outcome.excerpt.as_deref(), Some("went wrong\n"));
}

#[tokio::test]
async fn a_step_sees_only_its_inputs_so_other_edits_keep_its_cache() {
    let fx = Fixture::new().await;
    let plan = r#"
[[step]]
name = "only-src"
command = ["sh", "-c", "test -f src/a.rs && test ! -e docs/readme.md && test ! -e Cargo.toml"]
inputs = ["src/**"]
"#;
    let root = fx
        .tree(
            None,
            &[
                ("PLAN", plan),
                ("src/a.rs", "a"),
                ("docs/readme.md", "hi"),
                ("Cargo.toml", "[package]"),
            ],
        )
        .await;
    let first = fx.verify(root, None).await;
    assert!(first.green, "{:?}", first);

    let docs_edited = fx.tree(Some(root), &[("docs/readme.md", "changed")]).await;
    let second = fx.verify(docs_edited, None).await;
    assert!(second.steps[0].cached, "the input root ignores docs/");
    assert_eq!(second.steps[0].action, first.steps[0].action);

    let src_edited = fx.tree(Some(root), &[("src/a.rs", "b")]).await;
    let third = fx.verify(src_edited, None).await;
    assert!(third.green && !third.steps[0].cached);
}

#[tokio::test]
async fn shards_each_run_with_their_index() {
    let fx = Fixture::new().await;
    let plan = r#"
[[step]]
name = "tests"
command = ["sh", "-c", "test \"$TEST_TOTAL_SHARDS\" = 3 && test \"$TEST_SHARD_INDEX\" -lt 3"]
shards = 3
"#;
    let outcome = fx
        .verify(fx.tree(None, &[("PLAN", plan)]).await, None)
        .await;
    assert!(outcome.green, "{}", outcome.summary());
    let mut shards: Vec<_> = outcome.steps.iter().map(|s| s.shard.unwrap()).collect();
    shards.sort_unstable();
    assert_eq!(shards, [(0, 3), (1, 3), (2, 3)]);
    let actions: std::collections::HashSet<_> = outcome.steps.iter().map(|s| s.action).collect();
    assert_eq!(actions.len(), 3, "each shard is its own action");
}

#[tokio::test]
async fn a_working_directory_outside_the_inputs_is_created() {
    let fx = Fixture::new().await;
    let plan = r#"
[[step]]
command = ["sh", "-c", "test \"$(basename \"$PWD\")\" = out && test -f ../../src/a.rs && echo built > report.txt"]
inputs = ["src/**"]
working_dir = "build/out"
outputs = ["report.txt"]
"#;
    let outcome = fx
        .verify(
            fx.tree(None, &[("PLAN", plan), ("src/a.rs", "a")]).await,
            None,
        )
        .await;
    assert!(outcome.green, "{:?}", outcome);
}

#[tokio::test]
async fn files_larger_than_a_chunk_reach_the_worker_whole() {
    let fx = Fixture::new().await;
    let big: String = (0..300_000).map(|i| format!("{i:07}\n")).collect();
    let plan = format!(
        "[[step]]\ncommand = [\"sh\", \"-c\", \"test $(wc -c < big.txt) -eq {} && tail -n1 big.txt | grep -q 0299999\"]\n",
        big.len()
    );
    let root = fx.tree(None, &[("PLAN", &plan), ("big.txt", &big)]).await;
    let outcome = fx.verify(root, None).await;
    assert!(outcome.green, "{:?}", outcome);
}

#[tokio::test]
async fn a_step_past_its_timeout_times_out() {
    let fx = Fixture::new().await;
    let plan = "[[step]]\nname = \"slow\"\ncommand = [\"sleep\", \"30\"]\ntimeout_secs = 1\n";
    let outcome = fx
        .verify(fx.tree(None, &[("PLAN", plan)]).await, None)
        .await;
    assert!(!outcome.green);
    assert_eq!(outcome.steps[0].status, StepStatus::TimedOut);
}
