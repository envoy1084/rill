//! Running actions: the cache, merging identical requests, and taking an
//! action back from a worker that vanished.

use std::sync::atomic::Ordering;

use rill_reapi_proto::remoteworker::operation_queue_client::OperationQueueClient;
use rill_reapi_proto::remoteworker::{current_state, desired_state, CurrentState};

use super::fixtures::{digest, exec_clients, run, sync, upload_action, FakeWorker};
use crate::common::{connect, dev_config, TestServer};

#[tokio::test(flavor = "multi_thread")]
async fn an_action_runs_once_then_comes_from_the_cache() {
    let server = TestServer::start(dev_config()).await;
    let worker = FakeWorker::start(&server, None, "w1");
    let mut exec = exec_clients(&server, None).await;
    let action = upload_action(&mut exec, &["sh", "-c", "echo hi > out.txt"]).await;

    let first = run(&mut exec, &action).await;
    assert!(!first.cached_result);
    let result = first.result.expect("a result");
    assert_eq!(result.exit_code, 0);
    let output = format!("ran {}\n", action.hash);
    assert_eq!(
        result.output_files[0].digest,
        Some(digest(output.as_bytes()))
    );

    let again = run(&mut exec, &action).await;
    assert!(again.cached_result, "the second run is a cache hit");
    assert_eq!(again.result, Some(result));
    assert_eq!(worker.runs.load(Ordering::SeqCst), 1);
    worker.stop();
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn identical_concurrent_requests_share_one_run() {
    let server = TestServer::start(dev_config()).await;
    let mut exec = exec_clients(&server, None).await;
    let action = upload_action(&mut exec, &["make", "all"]).await;
    let mut waiting = Vec::new();
    for _ in 0..20 {
        let mut client = exec_clients(&server, None).await;
        let action = action.clone();
        waiting.push(tokio::spawn(async move { run(&mut client, &action).await }));
    }
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let worker = FakeWorker::start(&server, None, "w1");
    let mut responses = Vec::new();
    for task in waiting {
        responses.push(task.await.unwrap());
    }
    assert_eq!(responses.len(), 20);
    assert!(responses
        .iter()
        .all(|r| r.result.as_ref().is_some_and(|r| r.exit_code == 0)));
    assert_eq!(worker.runs.load(Ordering::SeqCst), 1, "merged into one run");
    worker.stop();
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn an_action_whose_worker_vanished_runs_on_another() {
    let mut config = dev_config();
    config.exec.lease_secs = 1;
    config.exec.idle_wait_secs = 1;
    let server = TestServer::start(config).await;
    let mut exec = exec_clients(&server, None).await;
    let action = upload_action(&mut exec, &["cargo", "test"]).await;
    let mut client = exec_clients(&server, None).await;
    let pending = tokio::spawn(async move { run(&mut client, &action).await });

    let mut lost = OperationQueueClient::new(connect(&server.grpc).await);
    let idle = CurrentState {
        worker_state: Some(current_state::WorkerState::Idle(())),
    };
    let taken = loop {
        let reply = lost
            .synchronize(sync("lost", idle.clone()))
            .await
            .unwrap()
            .into_inner();
        if let Some(desired_state::WorkerState::Executing(e)) =
            reply.desired_state.and_then(|d| d.worker_state)
        {
            break e;
        }
    };
    assert!(taken.action.is_some());

    tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
    assert_eq!(
        server.state.exec.reap().await.unwrap(),
        1,
        "the lapsed claim goes back"
    );
    let worker = FakeWorker::start(&server, None, "w2");
    let response = pending.await.unwrap();
    assert_eq!(response.result.unwrap().exit_code, 0);
    assert_eq!(worker.runs.load(Ordering::SeqCst), 1);
    worker.stop();
    server.stop().await;
}
