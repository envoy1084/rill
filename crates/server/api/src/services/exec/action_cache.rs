//! The action cache: results of actions already run. Only workers write
//! it, so a client cannot plant a result for an action it never ran.

use rill_exec::remote::BlobDigest;
use rill_reapi_proto::reapi::action_cache_server::ActionCache;
use rill_reapi_proto::reapi::{ActionResult, GetActionResultRequest, UpdateActionResultRequest};
use rill_runtime::error::Error;
use tonic::{Request, Response, Status};

use super::access::admit;
use super::service::ExecSvc;

#[tonic::async_trait]
impl ActionCache for ExecSvc {
    async fn get_action_result(
        &self,
        request: Request<GetActionResultRequest>,
    ) -> Result<Response<ActionResult>, Status> {
        let caller = admit(&self.state, &request, &request.get_ref().instance_name).await?;
        let digest = request
            .get_ref()
            .action_digest
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("action_digest is required"))?;
        let digest = BlobDigest::parse(digest).map_err(Error::from)?;
        match caller.space.result(&digest).await.map_err(Error::from)? {
            Some(result) => Ok(Response::new(result)),
            None => Err(Status::not_found(format!("no result for action {digest}"))),
        }
    }

    async fn update_action_result(
        &self,
        request: Request<UpdateActionResultRequest>,
    ) -> Result<Response<ActionResult>, Status> {
        let caller = admit(&self.state, &request, &request.get_ref().instance_name).await?;
        if !caller.worker {
            return Err(Status::permission_denied(
                "only remote execution workers write the action cache",
            ));
        }
        let request = request.into_inner();
        let digest = request
            .action_digest
            .as_ref()
            .ok_or_else(|| Status::invalid_argument("action_digest is required"))?;
        let digest = BlobDigest::parse(digest).map_err(Error::from)?;
        let result = request
            .action_result
            .ok_or_else(|| Status::invalid_argument("action_result is required"))?;
        caller
            .space
            .put_result(&digest, &result)
            .await
            .map_err(Error::from)?;
        Ok(Response::new(result))
    }
}
