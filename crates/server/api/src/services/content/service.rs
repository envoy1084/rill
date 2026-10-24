//! The `ContentService` impl: dispatches each RPC to its handler.

use std::sync::Arc;

use connectrpc::{InboundStream, RequestContext, Response, ServiceRequest};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ContentService;

use crate::services::common::GrpcResult;
use crate::services::repo::EventStream;
use crate::services::Call;
use rill_runtime::state::AppState;

/// The `ContentService`.
#[derive(Clone)]
pub struct ContentSvc {
    pub(super) state: Arc<AppState>,
    pub(super) peers: Arc<super::peers::SearchPeers>,
}

impl ContentSvc {
    /// A service over `state`.
    pub fn new(state: Arc<AppState>) -> Self {
        let peers = Arc::new(super::peers::SearchPeers::new(
            state.config.search.peer_ca_file.clone(),
        ));
        ContentSvc { state, peers }
    }
}

// Handlers return their concrete response types.
#[allow(refining_impl_trait)]
impl ContentService for ContentSvc {
    async fn read_file(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReadFileRequest>,
    ) -> GrpcResult<pb::ReadFileResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.read_file_impl(request).await?))
    }

    async fn batch_read_files(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::BatchReadFilesRequest>,
    ) -> GrpcResult<pb::BatchReadFilesResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.batch_read_files_impl(request).await?))
    }

    async fn stat(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::StatRequest>,
    ) -> GrpcResult<pb::StatResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.stat_impl(request).await?))
    }

    async fn list_directory(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListDirectoryRequest>,
    ) -> GrpcResult<pb::ListDirectoryResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_directory_impl(request).await?))
    }

    async fn get_tree(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GetTreeRequest>,
    ) -> GrpcResult<pb::GetTreeResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.get_tree_impl(request).await?))
    }

    async fn read_blob(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ReadBlobRequest>,
    ) -> GrpcResult<EventStream<pb::ReadBlobResponse>> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.read_blob_impl(request).await?))
    }

    async fn write_blob(
        &self,
        ctx: RequestContext,
        requests: InboundStream<pb::WriteBlobRequest>,
    ) -> GrpcResult<pb::WriteBlobResponse> {
        let request = Call::with_message(ctx, requests);
        Ok(Response::new(self.write_blob_impl(request).await?))
    }

    async fn batch_check_objects(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::BatchCheckObjectsRequest>,
    ) -> GrpcResult<pb::BatchCheckObjectsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.batch_check_objects_impl(request).await?))
    }

    async fn diff(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::DiffRequest>,
    ) -> GrpcResult<pb::DiffResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.diff_impl(request).await?))
    }

    async fn blame(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::BlameRequest>,
    ) -> GrpcResult<pb::BlameResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.blame_impl(request).await?))
    }

    async fn list_history(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListHistoryRequest>,
    ) -> GrpcResult<pb::ListHistoryResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_history_impl(request).await?))
    }

    async fn list_path_history(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListPathHistoryRequest>,
    ) -> GrpcResult<pb::ListPathHistoryResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_path_history_impl(request).await?))
    }

    async fn grep(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GrepRequest>,
    ) -> GrpcResult<pb::GrepResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.grep_impl(request).await?))
    }

    async fn grep_shards(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::GrepShardsRequest>,
    ) -> GrpcResult<pb::GrepShardsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.grep_shards_impl(request).await?))
    }

    async fn list_symbols(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::ListSymbolsRequest>,
    ) -> GrpcResult<pb::ListSymbolsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.list_symbols_impl(request).await?))
    }

    async fn find_definitions(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::FindDefinitionsRequest>,
    ) -> GrpcResult<pb::FindDefinitionsResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.find_definitions_impl(request).await?))
    }

    async fn find_references(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::FindReferencesRequest>,
    ) -> GrpcResult<pb::FindReferencesResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.find_references_impl(request).await?))
    }

    async fn upload_scip(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::UploadScipRequest>,
    ) -> GrpcResult<pb::UploadScipResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.upload_scip_impl(request).await?))
    }

    async fn find_merge_base(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::FindMergeBaseRequest>,
    ) -> GrpcResult<pb::FindMergeBaseResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.find_merge_base_impl(request).await?))
    }

    async fn is_ancestor(
        &self,
        ctx: RequestContext,
        request: ServiceRequest<'_, pb::IsAncestorRequest>,
    ) -> GrpcResult<pb::IsAncestorResponse> {
        let request = Call::new(ctx, request);
        Ok(Response::new(self.is_ancestor_impl(request).await?))
    }
}
