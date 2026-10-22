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
