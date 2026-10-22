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
        let response = self
            .state
            .exec
            .synchronize(request.into_inner())
            .await
            .map_err(Error::from)?;
        Ok(Response::new(response))
    }
}
