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
