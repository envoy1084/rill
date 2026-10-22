//! The `RepoService` gRPC impl: dispatches each RPC to its handler.

use std::sync::Arc;

use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::repo_service_server::RepoService;
use tonic::{Request, Response, Streaming};

use super::trees::BlobStream;
use super::watch::EventStream;
use crate::services::common::GrpcResult;

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

#[tonic::async_trait]
impl RepoService for RepoSvc {
    async fn get_repo(
        &self,
        request: Request<pb::GetRepoRequest>,
    ) -> GrpcResult<pb::GetRepoResponse> {
        Ok(Response::new(self.get_repo_impl(request).await?))
    }
    async fn list_repos(
        &self,
        request: Request<pb::ListReposRequest>,
    ) -> GrpcResult<pb::ListReposResponse> {
        Ok(Response::new(self.list_repos_impl(request).await?))
    }
    async fn create_repo(
        &self,
        request: Request<pb::CreateRepoRequest>,
    ) -> GrpcResult<pb::CreateRepoResponse> {
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_repo_impl(r).await?))
            })
            .await?,
        )
    }
    async fn get_change_version(
        &self,
        request: Request<pb::GetChangeVersionRequest>,
    ) -> GrpcResult<pb::GetChangeVersionResponse> {
        Ok(Response::new(self.get_change_version_impl(request).await?))
    }
    async fn get_change_info(
        &self,
        request: Request<pb::GetChangeInfoRequest>,
    ) -> GrpcResult<pb::GetChangeInfoResponse> {
        Ok(Response::new(self.get_change_info_impl(request).await?))
    }
    async fn get_change_versions(
        &self,
        request: Request<pb::GetChangeVersionsRequest>,
    ) -> GrpcResult<pb::GetChangeVersionsResponse> {
        Ok(Response::new(self.get_change_versions_impl(request).await?))
    }
    async fn log(&self, request: Request<pb::LogRequest>) -> GrpcResult<pb::LogResponse> {
        Ok(Response::new(self.log_impl(request).await?))
    }
    async fn path_history(
        &self,
        request: Request<pb::PathHistoryRequest>,
    ) -> GrpcResult<pb::PathHistoryResponse> {
        Ok(Response::new(self.path_history_impl(request).await?))
    }
    async fn blame(&self, request: Request<pb::BlameRequest>) -> GrpcResult<pb::BlameResponse> {
        Ok(Response::new(self.blame_impl(request).await?))
    }
    async fn grep(&self, request: Request<pb::GrepRequest>) -> GrpcResult<pb::GrepResponse> {
        Ok(Response::new(self.grep_impl(request).await?))
    }
    async fn get_tree(
        &self,
        request: Request<pb::GetTreeRequest>,
    ) -> GrpcResult<pb::GetTreeResponse> {
        Ok(Response::new(self.get_tree_impl(request).await?))
    }
    async fn list_dir(
        &self,
        request: Request<pb::ListDirRequest>,
    ) -> GrpcResult<pb::ListDirResponse> {
        Ok(Response::new(self.list_dir_impl(request).await?))
    }
    async fn resolve_path(
        &self,
        request: Request<pb::ResolvePathRequest>,
    ) -> GrpcResult<pb::ResolvePathResponse> {
        Ok(Response::new(self.resolve_path_impl(request).await?))
    }

    type GetBlobStream = BlobStream;

    async fn get_blob(
        &self,
        request: Request<pb::GetBlobRequest>,
    ) -> GrpcResult<Self::GetBlobStream> {
        Ok(Response::new(self.get_blob_impl(request).await?))
    }

    async fn put_blob(
        &self,
        request: Request<Streaming<pb::PutBlobRequest>>,
    ) -> GrpcResult<pb::PutBlobResponse> {
        Ok(Response::new(self.put_blob_impl(request).await?))
    }

    async fn has_objects(
        &self,
        request: Request<pb::HasObjectsRequest>,
    ) -> GrpcResult<pb::HasObjectsResponse> {
        Ok(Response::new(self.has_objects_impl(request).await?))
    }
    async fn diff(&self, request: Request<pb::DiffRequest>) -> GrpcResult<pb::DiffResponse> {
        Ok(Response::new(self.diff_impl(request).await?))
    }
    async fn merge_base(
        &self,
        request: Request<pb::MergeBaseRequest>,
    ) -> GrpcResult<pb::MergeBaseResponse> {
        Ok(Response::new(self.merge_base_impl(request).await?))
    }
    async fn is_ancestor(
        &self,
        request: Request<pb::IsAncestorRequest>,
    ) -> GrpcResult<pb::IsAncestorResponse> {
        Ok(Response::new(self.is_ancestor_impl(request).await?))
    }
    async fn create_change_version(
        &self,
        request: Request<pb::CreateChangeVersionRequest>,
    ) -> GrpcResult<pb::CreateChangeVersionResponse> {
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.create_change_version_impl(r).await?))
            })
            .await?,
        )
    }
    async fn apply_edits(
        &self,
        request: Request<pb::ApplyEditsRequest>,
    ) -> GrpcResult<pb::ApplyEditsResponse> {
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.apply_edits_impl(r).await?))
            })
            .await?,
        )
    }
    async fn rebase(&self, request: Request<pb::RebaseRequest>) -> GrpcResult<pb::RebaseResponse> {
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.rebase_impl(r).await?))
            })
            .await?,
        )
    }
    async fn merge(&self, request: Request<pb::MergeRequest>) -> GrpcResult<pb::MergeResponse> {
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.merge_impl(r).await?))
            })
            .await?,
        )
    }
    async fn resolve(
        &self,
        request: Request<pb::ResolveRequest>,
    ) -> GrpcResult<pb::ResolveResponse> {
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.resolve_impl(r).await?))
            })
            .await?,
        )
    }
    async fn resolve_ref(
        &self,
        request: Request<pb::ResolveRefRequest>,
    ) -> GrpcResult<pb::ResolveRefResponse> {
        Ok(Response::new(self.resolve_ref_impl(request).await?))
    }
    async fn list_refs(
        &self,
        request: Request<pb::ListRefsRequest>,
    ) -> GrpcResult<pb::ListRefsResponse> {
        Ok(Response::new(self.list_refs_impl(request).await?))
    }
    async fn move_ref(
        &self,
        request: Request<pb::MoveRefRequest>,
    ) -> GrpcResult<pb::MoveRefResponse> {
        Ok(
            crate::services::idem::once(&self.state, request, |r| async move {
                Ok(Response::new(self.move_ref_impl(r).await?))
            })
            .await?,
        )
    }
    async fn trunk_head(
        &self,
        request: Request<pb::TrunkHeadRequest>,
    ) -> GrpcResult<pb::TrunkHeadResponse> {
        Ok(Response::new(self.trunk_head_impl(request).await?))
    }
    async fn trunk_range(
        &self,
        request: Request<pb::TrunkRangeRequest>,
    ) -> GrpcResult<pb::TrunkRangeResponse> {
        Ok(Response::new(self.trunk_range_impl(request).await?))
    }

    type WatchRefsStream = EventStream<pb::RefEvent>;

    async fn watch_refs(
        &self,
        request: Request<pb::WatchRefsRequest>,
    ) -> GrpcResult<Self::WatchRefsStream> {
        Ok(Response::new(self.watch_refs_impl(request).await?))
    }

    type WatchTrunkStream = EventStream<pb::LandEvent>;

    async fn watch_trunk(
        &self,
        request: Request<pb::WatchTrunkRequest>,
    ) -> GrpcResult<Self::WatchTrunkStream> {
        Ok(Response::new(self.watch_trunk_impl(request).await?))
    }

    type WatchLogStream = EventStream<pb::LogEntry>;

    async fn watch_log(
        &self,
        request: Request<pb::WatchLogRequest>,
    ) -> GrpcResult<Self::WatchLogStream> {
        Ok(Response::new(self.watch_log_impl(request).await?))
    }

    type WatchEventsStream = EventStream<pb::EventEnvelope>;

    async fn watch_events(
        &self,
        request: Request<pb::WatchEventsRequest>,
    ) -> GrpcResult<Self::WatchEventsStream> {
        Ok(Response::new(self.watch_events_impl(request).await?))
    }

    async fn get_derived(
        &self,
        request: Request<pb::GetDerivedRequest>,
    ) -> GrpcResult<pb::GetDerivedResponse> {
        Ok(Response::new(self.get_derived_impl(request).await?))
    }
    async fn wait_derived(
        &self,
        request: Request<pb::WaitDerivedRequest>,
    ) -> GrpcResult<pb::WaitDerivedResponse> {
        Ok(Response::new(self.wait_derived_impl(request).await?))
    }
}
