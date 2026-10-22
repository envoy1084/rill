//! The `RepoService` gRPC impl: dispatches each RPC to its handler.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::RepoService;

use super::trees::BlobStream;
use super::watch::EventStream;
use crate::services::common::GrpcResult;

use crate::services::Call;
use connectrpc::{InboundStream, RequestContext, Response, ServiceRequest};
use rill_runtime::state::AppState;

/// The repository service.
#[derive(Clone)]
pub struct RepoSvc {
    pub(super) state: Arc<AppState>,
}

impl RepoSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        RepoSvc { state }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl RepoService for RepoSvc {
    async fn get_repo(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetRepoRequest>,
    ) -> GrpcResult<pb::GetRepoResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_repo_impl(request).await?))
    }
    async fn list_repos(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListReposRequest>,
    ) -> GrpcResult<pb::ListReposResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_repos_impl(request).await?))
    }
    async fn create_repo(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateRepoRequest>,
    ) -> GrpcResult<pb::CreateRepoResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_repo_impl(r).await?))
            })
            .await?,
        )
    }
    async fn get_change_version(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetChangeVersionRequest>,
    ) -> GrpcResult<pb::GetChangeVersionResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_change_version_impl(request).await?))
    }
    async fn get_change_info(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetChangeInfoRequest>,
    ) -> GrpcResult<pb::GetChangeInfoResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_change_info_impl(request).await?))
    }
    async fn get_change_versions(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetChangeVersionsRequest>,
    ) -> GrpcResult<pb::GetChangeVersionsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_change_versions_impl(request).await?))
    }
    async fn log(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::LogRequest>,
    ) -> GrpcResult<pb::LogResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.log_impl(request).await?))
    }
    async fn path_history(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::PathHistoryRequest>,
    ) -> GrpcResult<pb::PathHistoryResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.path_history_impl(request).await?))
    }
    async fn blame(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::BlameRequest>,
    ) -> GrpcResult<pb::BlameResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.blame_impl(request).await?))
    }
    async fn grep(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GrepRequest>,
    ) -> GrpcResult<pb::GrepResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.grep_impl(request).await?))
    }
    async fn get_tree(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetTreeRequest>,
    ) -> GrpcResult<pb::GetTreeResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_tree_impl(request).await?))
    }
    async fn list_dir(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListDirRequest>,
    ) -> GrpcResult<pb::ListDirResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_dir_impl(request).await?))
    }
    async fn resolve_path(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ResolvePathRequest>,
    ) -> GrpcResult<pb::ResolvePathResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.resolve_path_impl(request).await?))
    }

    async fn get_blob(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetBlobRequest>,
    ) -> GrpcResult<BlobStream> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_blob_impl(request).await?))
    }

    async fn put_blob(
        &self,
        ctx: RequestContext,
        requests: InboundStream<pb::PutBlobRequest>,
    ) -> GrpcResult<pb::PutBlobResponse> {
        let request = Call::with_message(ctx, requests);
        Ok(Response::new(self.put_blob_impl(request).await?))
    }

    async fn has_objects(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::HasObjectsRequest>,
    ) -> GrpcResult<pb::HasObjectsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.has_objects_impl(request).await?))
    }
    async fn diff(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DiffRequest>,
    ) -> GrpcResult<pb::DiffResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.diff_impl(request).await?))
    }
    async fn merge_base(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::MergeBaseRequest>,
    ) -> GrpcResult<pb::MergeBaseResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.merge_base_impl(request).await?))
    }
    async fn is_ancestor(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::IsAncestorRequest>,
    ) -> GrpcResult<pb::IsAncestorResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.is_ancestor_impl(request).await?))
    }
    async fn create_change_version(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CreateChangeVersionRequest>,
    ) -> GrpcResult<pb::CreateChangeVersionResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_change_version_impl(r).await?))
            })
            .await?,
        )
    }
    async fn apply_edits(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ApplyEditsRequest>,
    ) -> GrpcResult<pb::ApplyEditsResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.apply_edits_impl(r).await?))
            })
            .await?,
        )
    }
    async fn rebase(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::RebaseRequest>,
    ) -> GrpcResult<pb::RebaseResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.rebase_impl(r).await?))
            })
            .await?,
        )
    }
    async fn merge(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::MergeRequest>,
    ) -> GrpcResult<pb::MergeResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.merge_impl(r).await?))
            })
            .await?,
        )
    }
    async fn resolve(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ResolveRequest>,
    ) -> GrpcResult<pb::ResolveResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.resolve_impl(r).await?))
            })
            .await?,
        )
    }
    async fn resolve_ref(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ResolveRefRequest>,
    ) -> GrpcResult<pb::ResolveRefResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.resolve_ref_impl(request).await?))
    }
    async fn list_refs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListRefsRequest>,
    ) -> GrpcResult<pb::ListRefsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_refs_impl(request).await?))
    }
    async fn move_ref(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::MoveRefRequest>,
    ) -> GrpcResult<pb::MoveRefResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.move_ref_impl(r).await?))
            })
            .await?,
        )
    }
    async fn cut_release(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::CutReleaseRequest>,
    ) -> GrpcResult<pb::CutReleaseResponse> {
        let request = Call::new(ctx, request);
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.cut_release_impl(r).await?))
            })
            .await?,
        )
    }
    async fn trunk_head(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::TrunkHeadRequest>,
    ) -> GrpcResult<pb::TrunkHeadResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.trunk_head_impl(request).await?))
    }
    async fn trunk_range(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::TrunkRangeRequest>,
    ) -> GrpcResult<pb::TrunkRangeResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.trunk_range_impl(request).await?))
    }

    async fn watch_refs(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchRefsRequest>,
    ) -> GrpcResult<EventStream<pb::RefEvent>> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_refs_impl(request).await?))
    }

    async fn watch_trunk(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchTrunkRequest>,
    ) -> GrpcResult<EventStream<pb::LandEvent>> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_trunk_impl(request).await?))
    }

    async fn watch_log(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchLogRequest>,
    ) -> GrpcResult<EventStream<pb::LogEntry>> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_log_impl(request).await?))
    }

    async fn watch_events(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WatchEventsRequest>,
    ) -> GrpcResult<EventStream<pb::EventEnvelope>> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.watch_events_impl(request).await?))
    }

    async fn get_derived(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetDerivedRequest>,
    ) -> GrpcResult<pb::GetDerivedResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_derived_impl(request).await?))
    }
    async fn wait_derived(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::WaitDerivedRequest>,
    ) -> GrpcResult<pb::WaitDerivedResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.wait_derived_impl(request).await?))
    }
}
