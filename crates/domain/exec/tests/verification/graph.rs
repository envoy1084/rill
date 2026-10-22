//! A step given the build targets a change affects, from a graph query
//! run on the change and on its base.

use rill_exec::verify::StepStatus;

use crate::fixture::Fixture;

/// Two targets whose hashes are the contents of their source files.
const PLAN: &str = r#"
[graph]
command = ["sh", "-c", '''printf '{"//a:a": "%s", "//b:b": "%s"}' "$(cat a/src)" "$(cat b/src)" > target-hashes.json''']

[[step]]
name = "affected-tests"
command = ["sh", "-c", '''cat "$RILL_AFFECTED_TARGETS"; test "$(cat "$RILL_AFFECTED_TARGETS")" = //a:a''']
targets = "affected"
"#;

#[tokio::test]
async fn a_step_gets_only_the_targets_the_change_affects() {
    let fx = Fixture::new().await;
    let base = fx
        .tree(
            None,
            &[
                ("PLAN", PLAN),
                ("a/src", "1"),
                ("b/src", "1"),
                ("notes", "x"),
            ],
        )
        .await;

    let touches_a = fx.tree(Some(base), &[("a/src", "2")]).await;
    let outcome = fx.verify(touches_a, Some(base)).await;
    assert!(outcome.green, "{} {:?}", outcome.summary(), outcome.excerpt);
    assert_eq!(outcome.steps[0].status, StepStatus::Passed);

    let touches_no_target = fx.tree(Some(base), &[("notes", "y")]).await;
    let outcome = fx.verify(touches_no_target, Some(base)).await;
    assert_eq!(outcome.steps[0].status, StepStatus::Unaffected);
}

#[tokio::test]
async fn a_failing_graph_query_fails_verification() {
    let fx = Fixture::new().await;
    let plan = r#"
[graph]
command = ["sh", "-c", "echo no workspace >&2; exit 2"]

[[step]]
command = ["true"]
targets = "affected"
"#;
    let outcome = fx
        .verify(fx.tree(None, &[("PLAN", plan)]).await, None)
        .await;
    assert!(!outcome.green);
    assert!(
        outcome
            .summary()
            .starts_with("step \"build-graph\" failed with exit code 2"),
        "{}",
        outcome.summary()
    );
    assert_eq!(outcome.excerpt.as_deref(), Some("no workspace\n"));
}
