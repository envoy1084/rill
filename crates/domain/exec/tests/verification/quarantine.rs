//! Quarantined targets: a step failing only in them does not fail the
//! tree, and anything else failing still does.

use rill_exec::history::{Quarantine, TestHistory};
use rill_exec::verify::StepStatus;
use rill_types::{Principal, Timestamp};

use crate::fixture::Fixture;

/// A step writing a JUnit report with a failing `suite::flaky` case and,
/// with `real_failure`, a failing `suite::real` case too, then failing.
fn plan(real_failure: bool) -> String {
    let case = |name: &str| {
        format!(r#"<testcase name="{name}" classname="suite"><failure message="x"/></testcase>"#)
    };
    let mut cases = case("flaky");
    if real_failure {
        cases.push_str(&case("real"));
    }
    let report = format!(
        r#"<testsuites name="run"><testsuite name="suite">{cases}</testsuite></testsuites>"#
    );
    let script = format!("printf '%s' '{report}' > report.xml; exit 1");
    format!(
        "[[step]]\nname = \"unit\"\ncommand = [\"sh\", \"-c\", '''{script}''']\njunit = \"report.xml\"\n"
    )
}

async fn quarantine(fx: &Fixture, target: &str) {
    TestHistory::new(fx.repo.clone())
        .quarantine(&Quarantine {
            target: target.into(),
            since: Timestamp::now(),
            by: Principal::human("ann"),
            reason: "flaky".into(),
            intent: None,
        })
        .await
        .unwrap();
}

#[tokio::test]
async fn a_step_failing_only_in_quarantined_tests_does_not_fail_the_tree() {
    let fx = Fixture::new().await;
    let root = fx.tree(None, &[("PLAN", &plan(false))]).await;
    assert!(
        !fx.verify(root, None).await.green,
        "red before the quarantine"
    );

    quarantine(&fx, "test:suite::flaky").await;
    let outcome = fx.verify(root, None).await;
    assert!(outcome.green, "{}", outcome.summary());
    assert!(matches!(
        outcome.steps[0].status,
        StepStatus::Quarantined { exit_code: 1 }
    ));

    let both = fx.tree(None, &[("PLAN", &plan(true))]).await;
    assert!(
        !fx.verify(both, None).await.green,
        "a real failure still counts"
    );

    quarantine(&fx, "step:unit").await;
    assert!(
        fx.verify(both, None).await.green,
        "a quarantined step counts for nothing"
    );
}
