//! Who may execute and who may write the action cache.

use rill_policy::{Capability, Verb};
use rill_reapi_proto::reapi::{ActionResult, GetActionResultRequest, UpdateActionResultRequest};
use rill_types::{Principal, RepoId, RepoPathBuf, Timestamp};
use tonic::Code;

use super::fixtures::{exec_clients, upload_action};
use crate::common::TestServer;

fn config(secrets: &std::path::Path) -> rill_api::ServerConfig {
    let mut config = rill_api::ServerConfig::default();
    config.identity.keys = rill_identity::keys::KeyServiceConfig::Local {
        master_key_hex: Some("66".repeat(32)),
        master_key_file: None,
    };
    config.exec.worker_secret_file = Some(secrets.to_path_buf());
    config
}

#[tokio::test(flavor = "multi_thread")]
async fn only_workers_write_the_action_cache_and_narrow_tokens_need_execute() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join("workers");
    std::fs::write(&secrets, "s3cret\n").unwrap();
    let server = TestServer::start(config(&secrets)).await;
    let ann = server.token_for(&Principal::human("ann"));
    let mut exec = exec_clients(&server, Some(&ann)).await;
    let action = upload_action(&mut exec, &["true"]).await;
    let forged = UpdateActionResultRequest {
        action_digest: Some(action.clone()),
        action_result: Some(ActionResult {
            exit_code: 0,
            ..Default::default()
        }),
        ..Default::default()
    };
    let err = exec
        .cache
        .update_action_result(forged.clone())
        .await
        .unwrap_err();
    assert_eq!(
        err.code(),
        Code::PermissionDenied,
        "a client cannot plant a result"
    );

    let mut worker = exec_clients(&server, Some("rwk_s3cret")).await;
    worker.cache.update_action_result(forged).await.unwrap();
    let mut wrong = exec_clients(&server, Some("rwk_guess")).await;
    assert_eq!(
        wrong
            .cache
            .get_action_result(GetActionResultRequest {
                action_digest: Some(action.clone()),
                ..Default::default()
            })
            .await
            .unwrap_err()
            .code(),
        Code::Unauthenticated
    );

    let read_only = server.token_with(
        &Principal::human("ann"),
        &Capability {
            repo: RepoId::derived("default", "any"),
            workspace: None,
            paths: vec![RepoPathBuf::root()],
            verbs: vec![Verb::Read],
            expires_at: Timestamp::from_millis(Timestamp::now().millis() + 600_000),
        },
    );
    let mut narrow = exec_clients(&server, Some(&read_only)).await;
    let err = narrow
        .cache
        .get_action_result(GetActionResultRequest {
            action_digest: Some(action),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(
        err.code(),
        Code::PermissionDenied,
        "a token without execute"
    );
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_agent_runs_only_its_daily_executions() {
    let dir = tempfile::tempdir().unwrap();
    let secrets = dir.path().join("workers");
    std::fs::write(&secrets, "s3cret\n").unwrap();
    let stores = rill_store::Stores::memory();
    crate::common::register_bot(
        &stores,
        rill_types::identity::AgentBudgets {
            executions_per_day: Some(1),
            ..Default::default()
        },
    )
    .await;
    let server = TestServer::start_on(config(&secrets), stores).await;
    let bot = server.token_for(&Principal::agent("bot"));
    let mut exec = exec_clients(&server, Some(&bot)).await;
    let first = upload_action(&mut exec, &["first"]).await;
    let second = upload_action(&mut exec, &["second"]).await;
    let request = |action| rill_reapi_proto::reapi::ExecuteRequest {
        action_digest: Some(action),
        ..Default::default()
    };
    let queued = exec.execution.execute(request(first)).await.unwrap();
    let err = exec.execution.execute(request(second)).await.unwrap_err();
    assert_eq!(err.code(), Code::ResourceExhausted, "{err}");
    drop(queued);
    server.stop().await;
}
