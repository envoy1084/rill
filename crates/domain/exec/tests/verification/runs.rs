//! Verification runs: every step recorded with its logs and artifacts,
//! visible while it runs, and expired after the retention period.

use std::time::Duration;

use rill_exec::remote::BlobDigest;
use rill_exec::runs::{RunStore, StepEnd, StepState};
use rill_store::{Stores, DEFAULT_ORG};
use rill_types::ObjectHash;

use crate::fixture::Fixture;

const PLAN: &str = r#"
[[step]]
name = "build"
command = ["sh", "-c", "mkdir -p dist && echo binary > dist/app && echo built"]
outputs = ["dist/app"]

[[step]]
name = "docs"
command = ["true"]
inputs = ["docs/**"]
"#;

#[tokio::test]
async fn a_run_keeps_each_steps_logs_and_artifacts() {
    let fx = Fixture::new().await;
    let base = fx.tree(None, &[("PLAN", PLAN), ("src/a", "a")]).await;
    let root = fx.tree(Some(base), &[("src/a", "b")]).await;
    let outcome = fx.verify(root, Some(base)).await;
    assert!(outcome.green);

    let run = fx
        .runs
        .get(outcome.run.as_ref().unwrap())
        .await
        .unwrap()
        .unwrap();
    let end = run.header.end.as_ref().unwrap();
    assert!(
        end.green && end.summary.contains("1 passed"),
        "{}",
        end.summary
    );
    let state = |name: &str| {
        run.steps
            .iter()
            .find(|s| s.step.step == name)
            .map(|s| s.step.state.clone())
    };
    assert_eq!(state("build"), Some(StepState::Done(StepEnd::Passed)));
    assert_eq!(state("docs"), Some(StepState::Done(StepEnd::Unaffected)));

    let build = &run
        .steps
        .iter()
        .find(|s| s.step.step == "build")
        .unwrap()
        .step;
    let space = fx.scheduler.space(DEFAULT_ORG);
    let read = |digest: BlobDigest| {
        let space = space.clone();
        async move { space.read(&digest).await.unwrap().unwrap() }
    };
    assert_eq!(
        read(build.stdout.unwrap().into()).await.as_ref(),
        b"built\n"
    );
    assert_eq!(build.outputs[0].0, "dist/app");
    assert_eq!(read(build.outputs[0].1.into()).await.as_ref(), b"binary\n");
}

#[tokio::test]
async fn a_running_step_shows_as_running_until_it_ends() {
    let fx = Fixture::new().await;
    let plan = "[[step]]\nname = \"slow\"\ncommand = [\"sleep\", \"2\"]\n";
    let root = fx.tree(None, &[("PLAN", plan)]).await;
    let verifying = {
        let runner = &fx;
        async move { runner.verify(root, None).await }
    };
    let watching = async {
        loop {
            if let Some(header) = fx.runs.list(None, 1).await.unwrap().first() {
                let run = fx.runs.get(&header.id).await.unwrap().unwrap();
                if run.steps.first().map(|s| &s.step.state) == Some(&StepState::Running) {
                    assert!(run.header.end.is_none());
                    return header.id.clone();
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };
    let (outcome, seen) = tokio::join!(verifying, watching);
    assert_eq!(outcome.run, Some(seen.clone()));
    let run = fx.runs.get(&seen).await.unwrap().unwrap();
    assert_eq!(run.step(0).unwrap().state, StepState::Done(StepEnd::Passed));
}

#[tokio::test]
async fn runs_past_retention_are_removed_and_lists_are_newest_first() {
    let stores = Stores::memory();
    let repo = stores.repo(stores.create_repo("app", "").await.unwrap().id);
    let version = ObjectHash::digest(rill_types::ObjectKind::Blob, b"v");
    let keep = RunStore::new(repo.clone(), Duration::from_secs(3600));
    let first = keep.start(version, false).await.unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    let second = keep.start(version, true).await.unwrap();
    let listed: Vec<_> = keep
        .list(Some(&version), 10)
        .await
        .unwrap()
        .into_iter()
        .map(|h| h.id)
        .collect();
    assert_eq!(listed, [second.id.clone(), first.id.clone()]);

    tokio::time::sleep(Duration::from_millis(5)).await;
    let expire = RunStore::new(repo, Duration::ZERO);
    let third = expire.start(version, false).await.unwrap();
    let left: Vec<_> = expire
        .list(None, 10)
        .await
        .unwrap()
        .into_iter()
        .map(|h| h.id)
        .collect();
    assert_eq!(left, [third.id]);
    assert!(expire.get(&first.id).await.unwrap().is_none());
    assert!(expire.list(Some(&version), 10).await.unwrap().len() == 1);
}
