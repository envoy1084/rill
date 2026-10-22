//! REAPI clients, uploads and a fake worker speaking Buildbarn's protocol.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use futures::StreamExt;
use prost::Message;
use rill_reapi_proto::reapi::action_cache_client::ActionCacheClient;
use rill_reapi_proto::reapi::content_addressable_storage_client::ContentAddressableStorageClient;
use rill_reapi_proto::reapi::execution_client::ExecutionClient;
use rill_reapi_proto::reapi::{
    batch_update_blobs_request, digest_function, Action, ActionResult, BatchUpdateBlobsRequest,
    Command, Digest, Directory, ExecuteRequest, ExecuteResponse, OutputFile,
};
use rill_reapi_proto::remoteworker::operation_queue_client::OperationQueueClient;
use rill_reapi_proto::remoteworker::{
    current_state, desired_state, CurrentState, SynchronizeRequest,
};

use crate::common::{connect, Bearer, Svc, TestServer};

pub(crate) fn digest(bytes: &[u8]) -> Digest {
    Digest {
        hash: blake3::hash(bytes).to_hex().to_string(),
        size_bytes: bytes.len() as i64,
    }
}

pub(crate) struct Exec {
    pub(crate) cas: ContentAddressableStorageClient<Svc>,
    pub(crate) execution: ExecutionClient<Svc>,
    pub(crate) cache: ActionCacheClient<Svc>,
}

pub(crate) async fn exec_clients(server: &TestServer, token: Option<&str>) -> Exec {
    let channel = connect(&server.grpc).await;
    let svc = || {
        tonic::service::interceptor::InterceptedService::new(
            channel.clone(),
            Bearer(token.map(String::from)),
        )
    };
    Exec {
        cas: ContentAddressableStorageClient::new(svc()),
        execution: ExecutionClient::new(svc()),
        cache: ActionCacheClient::new(svc()),
    }
}

/// Upload `blobs` and return the digest of an action running `args`
/// that reads nothing and declares `out.txt` as its output.
pub(crate) async fn upload_action(exec: &mut Exec, args: &[&str]) -> Digest {
    let command = Command {
        arguments: args.iter().map(|a| a.to_string()).collect(),
        output_paths: vec!["out.txt".into()],
        ..Default::default()
    }
    .encode_to_vec();
    let root = Directory::default().encode_to_vec();
    let action = Action {
        command_digest: Some(digest(&command)),
        input_root_digest: Some(digest(&root)),
        ..Default::default()
    }
    .encode_to_vec();
    put(exec, &[&command, &root, &action]).await;
    digest(&action)
}

pub(crate) async fn put(exec: &mut Exec, blobs: &[&[u8]]) {
    let requests = blobs
        .iter()
        .map(|b| batch_update_blobs_request::Request {
            digest: Some(digest(b)),
            data: Bytes::copy_from_slice(b),
            compressor: 0,
        })
        .collect();
    let response = exec
        .cas
        .batch_update_blobs(BatchUpdateBlobsRequest {
            requests,
            digest_function: digest_function::Value::Blake3 as i32,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    assert!(response
        .responses
        .iter()
        .all(|r| r.status.as_ref().unwrap().code == 0));
}

/// Run `action` and wait for the final operation's response.
pub(crate) async fn run(exec: &mut Exec, action: &Digest) -> ExecuteResponse {
    let mut stream = exec
        .execution
        .execute(ExecuteRequest {
            action_digest: Some(action.clone()),
            digest_function: digest_function::Value::Blake3 as i32,
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner();
    while let Some(op) = stream.next().await {
        let op = op.unwrap();
        if op.done {
            let Some(rill_reapi_proto::longrunning::operation::Result::Response(any)) = op.result
            else {
                panic!("operation finished without a response: {op:?}");
            };
            return ExecuteResponse::decode(any.value.as_slice()).unwrap();
        }
    }
    panic!("the operation stream ended before the action finished");
}

/// A worker that "runs" every action by writing its first argument's
/// last word to `out.txt`, counting the actions it was given.
pub(crate) struct FakeWorker {
    pub(crate) runs: Arc<AtomicUsize>,
    task: tokio::task::JoinHandle<()>,
}

impl FakeWorker {
    pub(crate) fn start(server: &TestServer, token: Option<&str>, name: &str) -> Self {
        let runs = Arc::new(AtomicUsize::new(0));
        let counter = runs.clone();
        let endpoint = server.grpc.clone();
        let token = token.map(String::from);
        let name = name.to_string();
        let task = tokio::spawn(async move {
            let channel = connect(&endpoint).await;
            let svc = || {
                tonic::service::interceptor::InterceptedService::new(
                    channel.clone(),
                    Bearer(token.clone()),
                )
            };
            let mut queue = OperationQueueClient::new(svc());
            let mut cas = ContentAddressableStorageClient::new(svc());
            let mut state = CurrentState {
                worker_state: Some(current_state::WorkerState::Idle(())),
            };
            loop {
                let Ok(response) = queue.synchronize(sync(&name, state.clone())).await else {
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    continue;
                };
                let desired = response
                    .into_inner()
                    .desired_state
                    .and_then(|d| d.worker_state);
                state = match desired {
                    Some(desired_state::WorkerState::Executing(executing)) => {
                        counter.fetch_add(1, Ordering::SeqCst);
                        let output =
                            format!("ran {}\n", executing.action_digest.as_ref().unwrap().hash);
                        let mut exec = Exec {
                            cas: cas.clone(),
                            execution: ExecutionClient::new(svc()),
                            cache: ActionCacheClient::new(svc()),
                        };
                        put(&mut exec, &[output.as_bytes()]).await;
                        cas = exec.cas;
                        completed(executing.action_digest, output.as_bytes())
                    }
                    _ => CurrentState {
                        worker_state: Some(current_state::WorkerState::Idle(())),
                    },
                };
            }
        });
        FakeWorker { runs, task }
    }

    pub(crate) fn stop(self) {
        self.task.abort();
    }
}

pub(crate) fn sync(name: &str, state: CurrentState) -> SynchronizeRequest {
    SynchronizeRequest {
        worker_id: HashMap::from([("hostname".to_string(), name.to_string())]),
        current_state: Some(state),
        ..Default::default()
    }
}

fn completed(action: Option<Digest>, output: &[u8]) -> CurrentState {
    CurrentState {
        worker_state: Some(current_state::WorkerState::Executing(
            current_state::Executing {
                action_digest: action,
                execution_state: Some(current_state::executing::ExecutionState::Completed(
                    ExecuteResponse {
                        result: Some(ActionResult {
                            output_files: vec![OutputFile {
                                path: "out.txt".into(),
                                digest: Some(digest(output)),
                                ..Default::default()
                            }],
                            exit_code: 0,
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                )),
            },
        )),
    }
}
