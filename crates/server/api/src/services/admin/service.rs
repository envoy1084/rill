//! The `AdminService` gRPC impl: dispatches each RPC to its handler.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::AdminService;

use crate::services::common::{open, GrpcResult, Opened};
use crate::services::Call;
use crate::services::{idem, orgs};
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};
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

    pub(super) async fn open_named<T>(&self, request: &Call<T>, repo: &str) -> Result<Opened> {
        open(&self.state, request, repo).await
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl AdminService for AdminSvc {
    async fn who_am_i(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WhoAmIRequest>,
    ) -> GrpcResult<pb::WhoAmIResponse> {
        let request = Call::new(ctx, request);
        self.who_am_i_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    /// Mint a capability token. A caller holding a capability token may
    /// only delegate a narrower one (to itself or a sponsored agent); a
    /// caller without one must be an owner of the repository.
    async fn issue_token(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::IssueTokenRequest>,
    ) -> GrpcResult<pb::IssueTokenResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.issue_token_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn get_policy(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetPolicyRequest>,
    ) -> GrpcResult<pb::GetPolicyResponse> {
        let request = Call::new(ctx, request);
        self.get_policy_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }

    /// Append an imported version to trunk: the one way, besides landing,
    /// for history to reach trunk, so importers can bring existing history
    /// over. Needs the admin permit.
    async fn append_trunk(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::AppendTrunkRequest>,
    ) -> GrpcResult<pb::AppendTrunkResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.append_trunk_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    /// Validate, store and hot-swap a policy.
    async fn set_policy(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::SetPolicyRequest>,
    ) -> GrpcResult<pb::SetPolicyResponse> {
        let request = Call::new(ctx, request);
        idem::once(&self.state, request, |r| async move {
            self.set_policy_impl(r).await.map(Response::new)
        })
        .await
        .map_err(ConnectError::from)
    }

    async fn create_org(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateOrgRequest>,
    ) -> GrpcResult<pb::CreateOrgResponse> {
        let request = Call::new(ctx, request);
        Ok(idem::once(&self.state, request, |r| async move {
            Ok(Response::new(orgs::create(&self.state, r).await?))
        })
        .await?)
    }

    async fn list_orgs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListOrgsRequest>,
    ) -> GrpcResult<pb::ListOrgsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(orgs::list(&self.state, request).await?))
    }

    async fn health(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::HealthRequest>,
    ) -> GrpcResult<pb::HealthResponse> {
        let request = Call::new(ctx, request);
        self.health_impl(request)
            .await
            .map(Response::new)
            .map_err(ConnectError::from)
    }
}
