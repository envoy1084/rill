//! The worker protocol endpoint: Buildbarn workers report and receive
//! actions here.

use rill_reapi_proto::remoteworker::operation_queue_server::OperationQueue;
use rill_reapi_proto::remoteworker::{SynchronizeRequest, SynchronizeResponse};
use rill_runtime::error::Error;
use tonic::{Request, Response, Status};

use super::service::ExecSvc;
use crate::auth::is_worker;

#[tonic::async_trait]
impl OperationQueue for ExecSvc {
    async fn synchronize(
        &self,
        request: Request<SynchronizeRequest>,
    ) -> Result<Response<SynchronizeResponse>, Status> {
        if !self.state.config.exec.enabled {
            return Err(Status::unimplemented("remote execution is disabled"));
        }
        if !is_worker(&request) && !self.state.dev_mode() {
            return Err(Status::permission_denied("only workers synchronize"));
        }
        let request = request.into_inner();
        if let Some(outcome) = completion(&request) {
            self.state
                .metrics
                .exec_completions
                .get_or_create(&rill_runtime::telemetry::OutcomeLabels { outcome })
                .inc();
        }
        let response = self
            .state
            .exec
            .synchronize(request)
            .await
            .map_err(Error::from)?;
        Ok(Response::new(response))
    }
}

/// How a report's finished action ended, if it reports one.
fn completion(request: &SynchronizeRequest) -> Option<&'static str> {
    use rill_reapi_proto::remoteworker::current_state::{executing::ExecutionState, WorkerState};
    let Some(WorkerState::Executing(executing)) = request
        .current_state
        .as_ref()
        .and_then(|s| s.worker_state.as_ref())
    else {
        return None;
    };
    let Some(ExecutionState::Completed(response)) = &executing.execution_state else {
        return None;
    };
    Some(match (&response.status, &response.result) {
        (Some(status), _) if status.code != 0 => "error",
        (_, Some(result)) if result.exit_code == 0 => "ok",
        _ => "failed",
    })
}
