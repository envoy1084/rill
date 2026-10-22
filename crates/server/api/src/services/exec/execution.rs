//! Execute: serve an action from the cache, or queue it (joining an
//! identical one already queued) and stream its operation to the end.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures::Stream;
use prost::Message;
use rill_exec::remote::{
    parse_operation_name, priority_of, BlobDigest, ExecSpace, Stage, Submission,
};
use rill_reapi_proto::longrunning::Operation;
use rill_reapi_proto::reapi::execution_server::Execution;
use rill_reapi_proto::reapi::{Action, ExecuteRequest, ExecuteResponse, WaitExecutionRequest};
use rill_runtime::error::Error;
use rill_runtime::state::AppState;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status};

use super::access::{admit, spend_execution, ExecCaller};
use super::operations::{cached, operation_of};
use super::service::ExecSvc;

/// How long a stream waits for a change before repeating the operation,
/// which keeps idle connections open.
const KEEPALIVE: Duration = Duration::from_secs(30);

type OperationStream = Pin<Box<dyn Stream<Item = Result<Operation, Status>> + Send>>;

#[tonic::async_trait]
impl Execution for ExecSvc {
    type ExecuteStream = OperationStream;

    async fn execute(
        &self,
        request: Request<ExecuteRequest>,
    ) -> Result<Response<OperationStream>, Status> {
        let caller = admit(&self.state, &request, &request.get_ref().instance_name).await?;
        let label = action_label(request.metadata());
        let request = request.into_inner();
        let digest = request
            .action_digest
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("action_digest is required"))?;
        let digest = BlobDigest::parse(digest).map_err(Error::from)?;
        let action = load_action(&caller.space, &digest).await?;
        let nonhermetic = match &label {
            Some(label) => self
                .state
                .exec
                .nonhermetic(&caller.space, label)
                .await
                .map_err(Error::from)?
                .is_some(),
            None => false,
        };
        let lookup = !request.skip_cache_lookup && !action.do_not_cache && !nonhermetic;
        if lookup {
            let hit = self
                .state
                .exec
                .cached_result(&caller.space, &digest)
                .await
                .map_err(Error::from)?;
            if let Some(result) = hit {
                let response = ExecuteResponse {
                    result: Some(result),
                    cached_result: true,
                    ..Default::default()
                };
                let name = format!("{}/operations/cached-{}", caller.space.org, digest.hash);
                let op = cached(name, digest, response);
                self.count("cache_hit");
                return Ok(Response::new(Box::pin(futures::stream::iter([Ok(op)]))));
            }
        }
        check_inputs(&caller, &action).await?;
        spend_execution(&self.state, &caller.ctx).await?;
        let platform = match action.platform.clone() {
            Some(p) => Some(p),
            None => command_platform(&caller.space, &action).await?,
        };
        let submitted = self
            .state
            .exec
            .submit(
                &caller.space,
                Submission {
                    action: digest,
                    platform,
                    priority: priority_of(request.execution_policy.map_or(0, |p| p.priority)),
                    requester: caller.ctx.principal.to_string(),
                    do_not_cache: action.do_not_cache,
                    secrets: Vec::new(),
                    label,
                },
            )
            .await
            .map_err(Error::from)?;
        self.count(if submitted.merged { "merged" } else { "queued" });
        Ok(Response::new(follow(
            self.state.clone(),
            caller.space,
            submitted.operation.id,
        )))
    }

    type WaitExecutionStream = OperationStream;

    async fn wait_execution(
        &self,
        request: Request<WaitExecutionRequest>,
    ) -> Result<Response<OperationStream>, Status> {
        let name = request.get_ref().name.clone();
        let (org, id) = parse_operation_name(&name)
            .ok_or_else(|| Status::not_found(format!("operation {name:?}")))?;
        let caller = admit(&self.state, &request, org).await?;
        if self
            .state
            .exec
            .operation(&caller.space, id)
            .await
            .map_err(Error::from)?
            .is_none()
        {
            return Err(Status::not_found(format!("operation {name:?}")));
        }
        Ok(Response::new(follow(
            self.state.clone(),
            caller.space,
            id.to_string(),
        )))
    }
}

/// `FAILED_PRECONDITION` naming the missing blobs as REAPI asks, so the
/// client uploads them and retries.
fn missing(blobs: &[BlobDigest]) -> Status {
    use rill_reapi_proto::rpc::{precondition_failure::Violation, PreconditionFailure};
    let failure = PreconditionFailure {
        violations: blobs
            .iter()
            .map(|d| Violation {
                r#type: "MISSING".into(),
                subject: format!("blobs/{}/{}", d.hash, d.size),
                description: String::new(),
            })
            .collect(),
    };
    let message = format!("{} input blob(s) are not in the CAS", blobs.len());
    let status = rill_reapi_proto::rpc::Status {
        code: tonic::Code::FailedPrecondition as i32,
        message: message.clone(),
        details: vec![prost_types::Any {
            type_url: "type.googleapis.com/google.rpc.PreconditionFailure".into(),
            value: failure.encode_to_vec(),
        }],
    };
    Status::with_details(
        tonic::Code::FailedPrecondition,
        message,
        status.encode_to_vec().into(),
    )
}

async fn load_action(space: &ExecSpace, digest: &BlobDigest) -> Result<Action, Status> {
    let bytes = space
        .read(digest)
        .await
        .map_err(Error::from)?
        .ok_or_else(|| missing(&[*digest]))?;
    Action::decode(bytes).map_err(|e| Status::invalid_argument(format!("action {digest}: {e}")))
}

async fn check_inputs(caller: &ExecCaller, action: &Action) -> Result<(), Status> {
    let mut needed = Vec::new();
    for digest in [&action.command_digest, &action.input_root_digest]
        .into_iter()
        .flatten()
    {
        needed.push(BlobDigest::parse(digest).map_err(Error::from)?);
    }
    let absent = caller.space.missing(&needed).await.map_err(Error::from)?;
    if !absent.is_empty() {
        return Err(missing(&absent));
    }
    Ok(())
}

async fn command_platform(
    space: &ExecSpace,
    action: &Action,
) -> Result<Option<rill_reapi_proto::reapi::Platform>, Status> {
    let Some(digest) = &action.command_digest else {
        return Ok(None);
    };
    let digest = BlobDigest::parse(digest).map_err(Error::from)?;
    let Some(bytes) = space.read(&digest).await.map_err(Error::from)? else {
        return Ok(None);
    };
    #[allow(deprecated)]
    let platform = rill_reapi_proto::reapi::Command::decode(bytes)
        .map_err(|e| Status::invalid_argument(format!("command {digest}: {e}")))?
        .platform;
    Ok(platform)
}

/// Stream operation `id` as it changes, until it completes.
fn follow(state: Arc<AppState>, space: ExecSpace, id: String) -> OperationStream {
    let (tx, rx) = mpsc::channel(4);
    tokio::spawn(async move {
        let mut last = None;
        loop {
            let seen = match state.exec.operation_updates(&space, &id).await {
                Ok(seen) => seen,
                Err(e) => {
                    let _ = tx.send(Err(Status::from(Error::from(e)))).await;
                    return;
                }
            };
            let op = match state.exec.operation(&space, &id).await {
                Ok(Some(op)) => op,
                Ok(None) => {
                    let _ = tx
                        .send(Err(Status::not_found(format!("operation {id}"))))
                        .await;
                    return;
                }
                Err(e) => {
                    let _ = tx.send(Err(Status::from(Error::from(e)))).await;
                    return;
                }
            };
            let done = op.stage == Stage::Completed;
            if last != Some(op.stage) || done {
                last = Some(op.stage);
                if tx.send(Ok(operation_of(&op))).await.is_err() || done {
                    return;
                }
            }
            // A stopping server ends its streams; clients resume with
            // WaitExecution on another server.
            let waited = tokio::select! {
                _ = state.lifecycle.stop.cancelled() => return,
                waited = state.exec.wait_operation(&space, &id, seen, KEEPALIVE) => waited,
            };
            if let Err(e) = waited {
                let _ = tx.send(Err(Status::from(Error::from(e)))).await;
                return;
            }
            if tx.is_closed() {
                return;
            }
        }
    });
    Box::pin(ReceiverStream::new(rx))
}

/// Bazel's `RequestMetadata` header, as a hermeticity label:
/// `<tool>:<target>#<mnemonic>`, when it names a target.
fn action_label(metadata: &tonic::metadata::MetadataMap) -> Option<String> {
    let header = metadata.get_bin("build.bazel.remote.execution.v2.requestmetadata-bin")?;
    let bytes = header.to_bytes().ok()?;
    let meta = rill_reapi_proto::reapi::RequestMetadata::decode(bytes).ok()?;
    if meta.target_id.is_empty() {
        return None;
    }
    let tool = meta
        .tool_details
        .map(|t| t.tool_name)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "build".into());
    Some(format!(
        "{tool}:{}#{}",
        meta.target_id, meta.action_mnemonic
    ))
}
