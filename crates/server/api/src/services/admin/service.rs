//! The `AdminService` gRPC impl: dispatches each RPC to its handler.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::admin_service_server::AdminService;
use tonic::{Request, Response, Status};

use crate::services::common::{open, GrpcResult, Opened};
use crate::services::{idem, orgs};
use rill_runtime::error::Result;
use rill_runtime::state::AppState;

/// The admin service.
#[derive(Clone)]
pub struct AdminSvc {
    pub(super) state: Arc<AppState>,
}

impl AdminSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        AdminSvc { state }
    }

    pub(super) async fn open_named<T>(&self, request: &Request<T>, repo: &str) -> Result<Opened> {
        open(&self.state, request, repo).await
    }
}

#[tonic::async_trait]
impl AdminService for AdminSvc {
    async fn who_am_i(
        &self,
        request: Request<pb::WhoAmIRequest>,
    ) -> GrpcResult<pb::WhoAmIResponse> {
        self.who_am_i_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    /// Mint a capability token. A caller holding a capability token may
    /// only delegate a narrower one (to itself or a sponsored agent); a
    /// caller without one must be an owner of the repository.
    async fn issue_token(
        &self,
        request: Request<pb::IssueTokenRequest>,
    ) -> GrpcResult<pb::IssueTokenResponse> {
        idem::once(&self.state, request, |r| async move {
            self.issue_token_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn get_policy(
        &self,
        request: Request<pb::GetPolicyRequest>,
    ) -> GrpcResult<pb::GetPolicyResponse> {
        self.get_policy_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }

    /// Append an imported version to trunk: the one way, besides landing,
    /// for history to reach trunk, so importers can bring existing history
    /// over. Needs the admin permit.
    async fn append_trunk(
        &self,
        request: Request<pb::AppendTrunkRequest>,
    ) -> GrpcResult<pb::AppendTrunkResponse> {
        idem::once(&self.state, request, |r| async move {
            self.append_trunk_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    /// Validate, store and hot-swap a policy.
    async fn set_policy(
        &self,
        request: Request<pb::SetPolicyRequest>,
    ) -> GrpcResult<pb::SetPolicyResponse> {
        idem::once(&self.state, request, |r| async move {
            self.set_policy_impl(r).await.map(Response::new)
        })
        .await
        .map_err(Status::from)
    }

    async fn create_org(
        &self,
        request: Request<pb::CreateOrgRequest>,
    ) -> GrpcResult<pb::CreateOrgResponse> {
        Ok(idem::once(&self.state, request, |r| async move {
            Ok(Response::new(orgs::create(&self.state, r).await?))
        })
        .await?)
    }

    async fn list_orgs(
        &self,
        request: Request<pb::ListOrgsRequest>,
    ) -> GrpcResult<pb::ListOrgsResponse> {
        Ok(Response::new(orgs::list(&self.state, request).await?))
    }

    async fn health(&self, request: Request<pb::HealthRequest>) -> GrpcResult<pb::HealthResponse> {
        self.health_impl(request)
            .await
            .map(Response::new)
            .map_err(Status::from)
    }
}
