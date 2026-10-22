//! A change runs only the steps whose inputs it touched.

use rill_exec::verify::StepStatus;

use crate::fixture::Fixture;

const PLAN: &str = r#"
[[step]]
name = "rust"
command = ["true"]
inputs = ["src/**"]

[[step]]
name = "docs"
command = ["true"]
inputs = ["docs/**"]
"#;

#[tokio::test]
async fn steps_whose_inputs_did_not_change_are_not_run() {
    let fx = Fixture::new().await;
    let base = fx
        .tree(
            None,
            &[("PLAN", PLAN), ("src/a.rs", "a"), ("docs/a.md", "a")],
        )
        .await;
    let changed = fx.tree(Some(base), &[("docs/a.md", "b")]).await;

    let outcome = fx.verify(changed, Some(base)).await;
    assert!(outcome.green);
    let status = |name: &str| {
        outcome
            .steps
            .iter()
            .find(|s| s.step == name)
            .unwrap()
            .status
            .clone()
    };
    assert_eq!(status("rust"), StepStatus::Unaffected);
    assert_eq!(status("docs"), StepStatus::Passed);

    let plan_edited = fx
        .tree(Some(base), &[("PLAN", &format!("{PLAN}\n# edited\n"))])
        .await;
    let outcome = fx.verify(plan_edited, Some(base)).await;
    assert!(outcome.steps.iter().all(|s| s.status == StepStatus::Passed));
}
