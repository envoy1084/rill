//! Pre-land verification through the scheduler and an in-process worker:
//! a change that breaks its repository's plan does not land.

use super::*;
use crate::common::names;

const SECRET: &str = "orgs/default/secrets/DEPLOY_KEY";

const PLAN: &str = r#"
[[step]]
name = "no-bad-files"
command = ["sh", "-c", "if test -e src/bad; then echo found src/bad >&2; exit 1; fi"]
inputs = ["src/**"]
"#;

/// A change of `edits` on trunk, or on nothing in an empty repository.
async fn apply(c: &mut Clients, on_trunk: bool, edits: Vec<pb::Edit>) -> Bytes {
    c.changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: names::repo("app"),
            base: on_trunk.then(common::trunk_base).into(),
            edits,
            message: "edit".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .version
        .unwrap()
        .change_id
}

#[cfg(unix)]
#[tokio::test]
async fn a_change_that_fails_the_plan_does_not_land() {
    let mut config = dev_config();
    config.verify.enabled = true;
    config.exec.local.workers = 1;
    let server = TestServer::start(config).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "app".into(),
            ..Default::default()
        })
        .await
        .unwrap();

    let plan = apply(
        &mut c,
        false,
        vec![
            set_edit(".rill/verify.toml", PLAN.as_bytes()),
            set_edit("src/main.rs", b"fn main() {}"),
        ],
    )
    .await;
    // `.rill/` is policy: changing it needs an owner's approval.
    assert_eq!(land_with(&mut c, "app", plan, &[("Owner", 1)]).await, 1);

    let bad = apply(&mut c, true, vec![set_edit("src/bad", b"oops")]).await;
    let refused = try_land_with(&mut c, "app", bad, &[]).await.unwrap_err();
    assert!(
        refused.contains("\"no-bad-files\" failed with exit code 1")
            && refused.contains("found src/bad"),
        "{refused}"
    );

    let docs = apply(&mut c, true, vec![set_edit("docs/readme.md", b"hello")]).await;
    assert_eq!(land(&mut c, "app", docs).await, 2);
    server.stop().await;
}

#[cfg(unix)]
#[tokio::test]
async fn a_plan_step_gets_an_org_secret_set_through_the_api() {
    let mut config = dev_config();
    config.verify.enabled = true;
    config.exec.local.workers = 1;
    let server = TestServer::start(config).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "app".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let put = c
        .secrets
        .update_secret(pb::UpdateSecretRequest {
            secret: pb::Secret {
                name: SECRET.into(),
                repos: vec!["app".into()],
                ..Default::default()
            }
            .into(),
            value: Bytes::from_static(b"k3y-of-16-bytes!"),
            allow_missing: true,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .secret
        .unwrap();
    assert_eq!(
        (put.name.as_str(), put.repos.as_slice()),
        (SECRET, ["app".to_string()].as_slice())
    );
    let listed = c
        .secrets
        .list_secrets(pb::ListSecretsRequest {
            parent: names::org("default"),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .secrets;
    assert_eq!(listed.len(), 1);

    let plan = r#"
[[step]]
name = "deploy-check"
command = ["sh", "-c", 'test ${#DEPLOY_KEY} -eq 16']
secrets = ["DEPLOY_KEY"]
"#;
    let change = apply(
        &mut c,
        false,
        vec![set_edit(".rill/verify.toml", plan.as_bytes())],
    )
    .await;
    assert_eq!(land_with(&mut c, "app", change, &[("Owner", 1)]).await, 1);

    c.secrets
        .delete_secret(pb::DeleteSecretRequest {
            request_id: String::new(),
            name: SECRET.into(),
        })
        .await
        .unwrap();
    let next = apply(&mut c, true, vec![set_edit("README", b"hi")]).await;
    let refused = try_land_with(&mut c, "app", next, &[]).await.unwrap_err();
    assert!(
        refused.contains("secret DEPLOY_KEY is not set"),
        "{refused}"
    );
    server.stop().await;
}

#[cfg(unix)]
#[tokio::test]
async fn a_step_that_fails_then_passes_on_the_same_tree_is_listed_as_flaky() {
    use rill_proto::rill::v1::ChecksServiceClient;

    let mut config = dev_config();
    config.verify.enabled = true;
    config.exec.local.workers = 1;
    let server = TestServer::start(config).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "app".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    // Fails the first time it runs, passes after: a flake on one tree.
    let marker = std::env::temp_dir().join(format!(
        "rill-flaky-{}",
        hex::encode(rill_types::ids::random_id())
    ));
    let plan = format!(
        "[[step]]\nname = \"racy\"\ncommand = [\"sh\", \"-c\", \"test -e {0} || {{ touch {0}; exit 1; }}\"]\n",
        marker.display()
    );
    let change = apply(
        &mut c,
        false,
        vec![set_edit(".rill/verify.toml", plan.as_bytes())],
    )
    .await;
    let first = try_land_with(&mut c, "app", change.clone(), &[("Owner", 1)]).await;
    assert!(first.unwrap_err().contains("\"racy\" failed"));
    assert_eq!(common::request_land(&mut c, "app", change).await, Ok(1));
    let _ = std::fs::remove_file(&marker);

    let channel = connect(&server.grpc).await;
    let checks = ChecksServiceClient::authed(channel, Bearer(None));
    let flaky = checks
        .list_flaky_tests(pb::ListFlakyTestsRequest {
            parent: names::repo("app"),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .tests;
    assert_eq!(flaky.len(), 1, "{flaky:?}");
    assert_eq!(flaky[0].target, "step:racy");
    let history = checks
        .get_test_history(pb::GetTestHistoryRequest {
            parent: names::repo("app"),
            target: "step:racy".into(),
            max_results: 0,
        })
        .await
        .unwrap()
        .into_owned();
    let outcomes: Vec<_> = history.runs.iter().map(|r| r.outcome.to_i32()).collect();
    assert_eq!(
        outcomes,
        [
            pb::TestOutcome::Passed as i32,
            pb::TestOutcome::Failed as i32
        ]
    );
    server.stop().await;
}

#[cfg(unix)]
#[tokio::test]
async fn a_failed_runs_steps_logs_and_artifacts_are_readable() {
    use futures::StreamExt;
    use rill_proto::rill::v1::ChecksServiceClient;

    let mut config = dev_config();
    config.verify.enabled = true;
    config.exec.local.workers = 2;
    let server = TestServer::start(config).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "app".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let plan = r#"
[[step]]
name = "build"
command = ["sh", "-c", "mkdir -p out && printf 'artifact bytes' > out/app && echo built"]
outputs = ["out/app"]

[[step]]
name = "check"
command = ["sh", "-c", "sleep 1; echo 'expected 2, got 3' >&2; exit 1"]
"#;
    let change = apply(
        &mut c,
        false,
        vec![set_edit(".rill/verify.toml", plan.as_bytes())],
    )
    .await;
    let refused = try_land_with(&mut c, "app", change, &[("Owner", 1)])
        .await
        .unwrap_err();
    let run_id = refused
        .split("(run ")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .unwrap_or_else(|| panic!("no run in {refused:?}"))
        .to_string();

    let checks = ChecksServiceClient::authed(connect(&server.grpc).await, Bearer(None));
    let run_name = format!("{}/runs/{run_id}", names::repo("app"));
    let run = checks
        .get_run(pb::GetRunRequest {
            name: run_name.clone(),
        })
        .await
        .unwrap()
        .into_owned()
        .run
        .unwrap();
    assert_eq!(run.green, Some(false));
    let step = |name: &str| run.steps.iter().find(|s| s.name == name).unwrap().clone();
    let (build, check) = (step("build"), step("check"));
    assert_eq!(build.state, pb::StepState::Passed);
    assert_eq!(
        (check.state.to_i32(), check.exit_code),
        (pb::StepState::Failed as i32, Some(1))
    );

    let log = |step: u32, stream: pb::LogStream, offset: i64| pb::ReadLogRequest {
        run: run_name.clone(),
        step,
        stream: stream.into(),
        offset,
        limit: 0,
    };
    let stderr = checks
        .read_log(log(check.index, pb::LogStream::Stderr, 0))
        .await
        .unwrap()
        .into_owned();
    assert_eq!(stderr.data.as_ref(), b"expected 2, got 3\n");
    let tail = checks
        .read_log(log(check.index, pb::LogStream::Stderr, -3))
        .await
        .unwrap()
        .into_owned();
    assert_eq!(
        (tail.data.as_ref(), tail.offset, tail.size),
        (&b" 3\n"[..], 15, 18)
    );
    let artifact = checks
        .read_artifact(pb::ReadArtifactRequest {
            run: run_name.clone(),
            step: build.index,
            path: "out/app".into(),
            offset: 0,
            limit: 0,
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(artifact.data.as_ref(), b"artifact bytes");

    let listed = checks
        .list_runs(pb::ListRunsRequest {
            parent: names::repo("app"),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .runs;
    assert_eq!(listed[0].id, run_id);
    let watched: Vec<_> = checks
        .watch_run(pb::WatchRunRequest { name: run_name })
        .await
        .unwrap()
        .messages()
        .collect()
        .await;
    assert_eq!(watched.len(), 1, "a finished run is sent once");
    server.stop().await;
}

#[cfg(unix)]
#[tokio::test]
async fn a_step_whose_rerun_differs_is_listed_as_nonhermetic() {
    use rill_proto::rill::v1::ChecksServiceClient;

    let mut config = dev_config();
    config.verify.enabled = true;
    config.exec.local.workers = 1;
    config.exec.probe_per_mille = 1000;
    let server = TestServer::start(config).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "app".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let plan = "[[step]]\nname = \"stamp\"\ncommand = [\"sh\", \"-c\", \"od -An -N8 -tx8 /dev/urandom > stamp\"]\noutputs = [\"stamp\"]\n";
    let change = apply(
        &mut c,
        false,
        vec![set_edit(".rill/verify.toml", plan.as_bytes())],
    )
    .await;
    assert_eq!(land_with(&mut c, "app", change, &[("Owner", 1)]).await, 1);

    let checks = ChecksServiceClient::authed(connect(&server.grpc).await, Bearer(None));
    let mut steps = Vec::new();
    for _ in 0..100 {
        steps = checks
            .list_nonhermetic_steps(pb::ListNonhermeticStepsRequest {
                parent: names::repo("app"),
                ..Default::default()
            })
            .await
            .unwrap()
            .into_owned()
            .steps;
        if !steps.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(steps.len(), 1, "the probe never flagged the step");
    assert_eq!(
        (steps[0].step.as_str(), steps[0].differences.as_slice()),
        ("stamp", ["stamp differs".to_string()].as_slice())
    );
    server.stop().await;
}

#[tokio::test]
async fn an_owner_quarantines_a_target_and_lifts_it_again() {
    use rill_proto::rill::v1::ChecksServiceClient;
    let server = TestServer::start(dev_config()).await;
    let c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "app".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let checks = ChecksServiceClient::authed(connect(&server.grpc).await, Bearer(None));
    let quarantine = |target: &str| pb::CreateQuarantineRequest {
        request_id: String::new(),
        parent: names::repo("app"),
        quarantine: pb::Quarantine {
            target: target.into(),
            reason: "flips on unchanged inputs".into(),
            ..Default::default()
        }
        .into(),
    };
    let bad = checks
        .create_quarantine(quarantine("unit"))
        .await
        .unwrap_err();
    assert_eq!(bad.code, connectrpc::ErrorCode::InvalidArgument);
    let added = checks
        .create_quarantine(quarantine("test:core::racy"))
        .await
        .unwrap()
        .into_owned()
        .quarantine
        .unwrap();
    let again = checks
        .create_quarantine(quarantine("test:core::racy"))
        .await
        .unwrap_err();
    assert_eq!(again.code, connectrpc::ErrorCode::AlreadyExists);
    let listed = checks
        .list_quarantines(pb::ListQuarantinesRequest {
            parent: names::repo("app"),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .quarantines;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].reason, "flips on unchanged inputs");
    let release = || pb::DeleteQuarantineRequest {
        request_id: String::new(),
        name: added.name.clone(),
    };
    checks.delete_quarantine(release()).await.unwrap();
    let missing = checks.delete_quarantine(release()).await.unwrap_err();
    assert_eq!(missing.code, connectrpc::ErrorCode::NotFound);
}

#[tokio::test]
async fn an_owner_freezes_trunk_and_its_health_says_so() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "app".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let lander =
        rill_proto::rill::v1::TrunkServiceClient::authed(connect(&server.grpc).await, Bearer(None));
    let health = |_: &mut Clients| {
        let lander = lander.clone();
        async move {
            lander
                .get_trunk_health(pb::GetTrunkHealthRequest {
                    parent: names::repo("app"),
                })
                .await
                .unwrap()
                .into_owned()
        }
    };
    let fresh = health(&mut c).await;
    assert_eq!(fresh.health.unwrap().state, "green");
    assert!(fresh.open.is_empty());

    let frozen = lander
        .clone()
        .freeze_trunk(pb::FreezeTrunkRequest {
            request_id: String::new(),
            parent: names::repo("app"),
            reason: "incident review".into(),
        })
        .await
        .unwrap()
        .into_owned();
    assert!(frozen.health.unwrap().frozen);
    let now = health(&mut c).await.health.unwrap();
    assert_eq!(
        (now.frozen, now.frozen_reason.as_deref()),
        (true, Some("incident review"))
    );
    lander
        .clone()
        .unfreeze_trunk(pb::UnfreezeTrunkRequest {
            request_id: String::new(),
            parent: names::repo("app"),
        })
        .await
        .unwrap();
    assert!(!health(&mut c).await.health.unwrap().frozen);
    let incidents = lander
        .clone()
        .list_incidents(pb::ListIncidentsRequest {
            parent: names::repo("app"),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .incidents;
    assert!(incidents.is_empty());
}

#[tokio::test]
async fn an_owner_previews_and_reverts_everything_an_author_landed() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: names::org("default"),
            repo_id: "app".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let change = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: names::repo("app"),
            edits: vec![set_edit("agent.txt", b"done by an agent\n")],
            message: "agent work".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .version
        .unwrap()
        .change_id;
    land(&mut c, "app", change).await;

    let lander =
        rill_proto::rill::v1::TrunkServiceClient::authed(connect(&server.grpc).await, Bearer(None));
    // Sessions and models come from tokens; a dev caller has none.
    let by_dev = pb::RevertSelector {
        author: Some("dev".into()),
        ..Default::default()
    };
    let preview = |selector: pb::RevertSelector| pb::PreviewBulkRevertRequest {
        parent: names::repo("app"),
        selector: selector.into(),
    };
    let unnamed = lander
        .preview_bulk_revert(preview(pb::RevertSelector {
            since: Some(0),
            ..by_dev.clone()
        }))
        .await;
    assert!(unnamed.is_ok(), "an author is named");
    let range_only = lander
        .preview_bulk_revert(preview(pb::RevertSelector {
            since: Some(0),
            ..Default::default()
        }))
        .await
        .unwrap_err();
    assert_eq!(range_only.code, connectrpc::ErrorCode::InvalidArgument);

    let previewed = lander
        .preview_bulk_revert(preview(by_dev.clone()))
        .await
        .unwrap()
        .into_owned();
    assert_eq!(previewed.entries.len(), 1);
    assert_eq!(
        (
            previewed.entries[0].position,
            previewed.entries[0].land_request.as_str()
        ),
        (1, "")
    );
    let operation = lander
        .bulk_revert(pb::BulkRevertRequest {
            request_id: String::new(),
            parent: names::repo("app"),
            selector: by_dev.into(),
            reason: "session went wrong".into(),
        })
        .await
        .unwrap()
        .into_owned()
        .operation
        .unwrap();
    assert!(operation.done);
    let applied = match operation.result {
        Some(pb::operation::Result::BulkRevert(result)) => result,
        other => panic!("{other:?}"),
    };
    let ticket = applied.entries[0].land_request.clone();
    for _ in 0..100 {
        let status = c
            .landing
            .get_land_request(pb::GetLandRequestRequest {
                name: ticket.clone(),
            })
            .await
            .unwrap()
            .into_owned()
            .land_request
            .unwrap();
        if status.trunk_position.is_some() {
            assert_eq!(status.trunk_position, Some(2));
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("the revert did not land");
}
