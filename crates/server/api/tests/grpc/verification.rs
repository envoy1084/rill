//! Pre-land verification through the scheduler and an in-process worker:
//! a change that breaks its repository's plan does not land.

use super::*;

const PLAN: &str = r#"
[[step]]
name = "no-bad-files"
command = ["sh", "-c", "if test -e src/bad; then echo found src/bad >&2; exit 1; fi"]
inputs = ["src/**"]
"#;

/// A change of `edits` on trunk, or on nothing in an empty repository.
async fn apply(c: &mut Clients, on_trunk: bool, edits: Vec<pb::Edit>) -> Bytes {
    c.repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            base: on_trunk.then(|| pb::EditBase {
                base: Some(pb::edit_base::Base::Root(trunk())),
            }),
            edits,
            message: "edit".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner()
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
            operation_id: Bytes::new(),
            repo: "app".into(),
            description: None,
            windows_safe: false,
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
            operation_id: Bytes::new(),
            repo: "app".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let put = c
        .secrets
        .put_secret(pb::PutSecretRequest {
            operation_id: Bytes::new(),
            org: String::new(),
            name: "DEPLOY_KEY".into(),
            value: Bytes::from_static(b"k3y-of-16-bytes!"),
            repos: vec!["app".into()],
            trunk_only: false,
        })
        .await
        .unwrap()
        .into_inner()
        .secret
        .unwrap();
    assert_eq!(
        (put.name.as_str(), put.repos.as_slice()),
        ("DEPLOY_KEY", ["app".to_string()].as_slice())
    );
    let listed = c
        .secrets
        .list_secrets(pb::ListSecretsRequest { org: String::new() })
        .await
        .unwrap()
        .into_inner()
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

    let deleted = c
        .secrets
        .delete_secret(pb::DeleteSecretRequest {
            operation_id: Bytes::new(),
            org: String::new(),
            name: "DEPLOY_KEY".into(),
        })
        .await
        .unwrap()
        .into_inner()
        .deleted;
    assert!(deleted);
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
    use rill_proto::rill::v1::checks_service_client::ChecksServiceClient;

    let mut config = dev_config();
    config.verify.enabled = true;
    config.exec.local.workers = 1;
    let server = TestServer::start(config).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            description: None,
            windows_safe: false,
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
    let mut checks = ChecksServiceClient::new(channel);
    let flaky = checks
        .list_flaky_tests(pb::ListFlakyTestsRequest {
            repo: "app".into(),
            min_score: None,
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner()
        .tests;
    assert_eq!(flaky.len(), 1, "{flaky:?}");
    assert_eq!(flaky[0].target, "step:racy");
    let history = checks
        .get_test_history(pb::GetTestHistoryRequest {
            repo: "app".into(),
            target: "step:racy".into(),
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner();
    let outcomes: Vec<_> = history.runs.iter().map(|r| r.outcome).collect();
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
    use rill_proto::rill::v1::checks_service_client::ChecksServiceClient;

    let mut config = dev_config();
    config.verify.enabled = true;
    config.exec.local.workers = 2;
    let server = TestServer::start(config).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            description: None,
            windows_safe: false,
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

    let mut checks = ChecksServiceClient::new(connect(&server.grpc).await);
    let get = pb::GetRunRequest {
        repo: "app".into(),
        run: run_id.clone(),
    };
    let run = checks.get_run(get.clone()).await.unwrap().into_inner();
    assert_eq!(run.green, Some(false));
    let step = |name: &str| run.steps.iter().find(|s| s.name == name).unwrap().clone();
    let (build, check) = (step("build"), step("check"));
    assert_eq!(build.state, pb::StepState::Passed as i32);
    assert_eq!(
        (check.state, check.exit_code),
        (pb::StepState::Failed as i32, Some(1))
    );

    let log = |step: u32, stream: pb::LogStream, offset: i64| pb::ReadLogRequest {
        repo: "app".into(),
        run: run_id.clone(),
        step,
        stream: stream as i32,
        offset,
        limit: 0,
    };
    let stderr = checks
        .read_log(log(check.index, pb::LogStream::Stderr, 0))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(stderr.data.as_ref(), b"expected 2, got 3\n");
    let tail = checks
        .read_log(log(check.index, pb::LogStream::Stderr, -3))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(
        (tail.data.as_ref(), tail.offset, tail.size),
        (&b" 3\n"[..], 15, 18)
    );
    let artifact = checks
        .read_artifact(pb::ReadArtifactRequest {
            repo: "app".into(),
            run: run_id.clone(),
            step: build.index,
            path: "out/app".into(),
            offset: 0,
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(artifact.data.as_ref(), b"artifact bytes");

    let listed = checks
        .list_runs(pb::ListRunsRequest {
            repo: "app".into(),
            version: Bytes::new(),
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner()
        .runs;
    assert_eq!(listed[0].id, run_id);
    let watched: Vec<_> = checks
        .watch_run(get)
        .await
        .unwrap()
        .into_inner()
        .collect()
        .await;
    assert_eq!(watched.len(), 1, "a finished run is sent once");
    server.stop().await;
}

#[cfg(unix)]
#[tokio::test]
async fn a_step_whose_rerun_differs_is_listed_as_nonhermetic() {
    use rill_proto::rill::v1::checks_service_client::ChecksServiceClient;

    let mut config = dev_config();
    config.verify.enabled = true;
    config.exec.local.workers = 1;
    config.exec.probe_per_mille = 1000;
    let server = TestServer::start(config).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            description: None,
            windows_safe: false,
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

    let mut checks = ChecksServiceClient::new(connect(&server.grpc).await);
    let mut steps = Vec::new();
    for _ in 0..100 {
        steps = checks
            .list_nonhermetic(pb::ListNonhermeticRequest { repo: "app".into() })
            .await
            .unwrap()
            .into_inner()
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
    use rill_proto::rill::v1::checks_service_client::ChecksServiceClient;
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let mut checks = ChecksServiceClient::new(connect(&server.grpc).await);
    let quarantine = |target: &str| pb::QuarantineRequest {
        operation_id: Bytes::new(),
        repo: "app".into(),
        target: target.into(),
        reason: "flips on unchanged inputs".into(),
    };
    let bad = checks.quarantine(quarantine("unit")).await.unwrap_err();
    assert_eq!(bad.code(), tonic::Code::InvalidArgument);
    for added in [true, false] {
        let reply = checks
            .quarantine(quarantine("test:core::racy"))
            .await
            .unwrap();
        assert_eq!(reply.into_inner().added, added);
    }
    let listed = checks
        .list_quarantined(pb::ListQuarantinedRequest { repo: "app".into() })
        .await
        .unwrap()
        .into_inner()
        .targets;
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].reason, "flips on unchanged inputs");
    let release = || pb::ReleaseQuarantineRequest {
        operation_id: Bytes::new(),
        repo: "app".into(),
        target: "test:core::racy".into(),
    };
    assert!(
        checks
            .release_quarantine(release())
            .await
            .unwrap()
            .into_inner()
            .released
    );
    assert!(
        !checks
            .release_quarantine(release())
            .await
            .unwrap()
            .into_inner()
            .released
    );
}

#[tokio::test]
async fn an_owner_freezes_trunk_and_its_health_says_so() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let lander = rill_proto::rill::v1::lander_service_client::LanderServiceClient::new(
        connect(&server.grpc).await,
    );
    let health = |_: &mut Clients| {
        let mut lander = lander.clone();
        async move {
            lander
                .get_trunk_health(pb::GetTrunkHealthRequest { repo: "app".into() })
                .await
                .unwrap()
                .into_inner()
        }
    };
    let fresh = health(&mut c).await;
    assert_eq!(fresh.health.unwrap().state, "green");
    assert!(fresh.open.is_empty());

    let freeze = |frozen| pb::FreezeTrunkRequest {
        operation_id: Bytes::new(),
        repo: "app".into(),
        frozen,
        reason: "incident review".into(),
    };
    let frozen = lander
        .clone()
        .freeze_trunk(freeze(true))
        .await
        .unwrap()
        .into_inner();
    assert!(frozen.health.unwrap().frozen);
    let now = health(&mut c).await.health.unwrap();
    assert_eq!(
        (now.frozen, now.frozen_reason.as_deref()),
        (true, Some("incident review"))
    );
    lander.clone().freeze_trunk(freeze(false)).await.unwrap();
    assert!(!health(&mut c).await.health.unwrap().frozen);
    let incidents = lander
        .clone()
        .list_incidents(pb::ListIncidentsRequest {
            repo: "app".into(),
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner()
        .incidents;
    assert!(incidents.is_empty());
}
