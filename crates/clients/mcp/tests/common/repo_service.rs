//! `RepoService` implementation backed by [`MockRepo`].

use connectrpc::{RequestContext, Response, ServiceRequest, ServiceStream};
use rill_proto::convert::id_to_proto;
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::RepoService;

use super::mock_repo::MockRepo;
use super::rpc::{unimplemented_rpcs, Request, RpcResult};

// The mocks return concrete types.
#[allow(refining_impl_trait)]
impl RepoService for MockRepo {
    async fn get_repo(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::GetRepoRequest>,
    ) -> RpcResult<pb::GetRepoResponse> {
        let name = Request::new(ctx, req).into_inner().name;
        let short = name.rsplit('/').next().unwrap_or_default().to_string();
        Ok(Response::new(pb::GetRepoResponse {
            repo: (pb::Repo {
                uid: id_to_proto(&rill_types::RepoId::derived("test", &short)),
                name,
                repo_id: short,
                description: Some("mock".into()),
                created_at: 1,
                trunk_head_position: 3,
                windows_safe: false,
                org: "default".into(),
            })
            .into(),
        }))
    }

    unimplemented_rpcs! {
        list_repos(pb::ListReposRequest) -> pb::ListReposResponse;
        create_repo(pb::CreateRepoRequest) -> pb::CreateRepoResponse;
        resolve_revision(pb::ResolveRevisionRequest) -> pb::ResolveRevisionResponse;
        get_ref(pb::GetRefRequest) -> pb::GetRefResponse;
        list_refs(pb::ListRefsRequest) -> pb::ListRefsResponse;
        update_ref(pb::UpdateRefRequest) -> pb::UpdateRefResponse;
        delete_ref(pb::DeleteRefRequest) -> pb::DeleteRefResponse;
        create_release(pb::CreateReleaseRequest) -> pb::CreateReleaseResponse;
        get_trunk_position(pb::GetTrunkPositionRequest) -> pb::GetTrunkPositionResponse;
        list_trunk_positions(pb::ListTrunkPositionsRequest) -> pb::ListTrunkPositionsResponse;
        watch_trunk(pb::WatchTrunkRequest) -> ServiceStream<pb::WatchTrunkResponse>;
        watch_refs(pb::WatchRefsRequest) -> ServiceStream<pb::WatchRefsResponse>;
        watch_versions(pb::WatchVersionsRequest) -> ServiceStream<pb::WatchVersionsResponse>;
        get_derived_value(pb::GetDerivedValueRequest) -> pb::GetDerivedValueResponse;
        wait_derived_value(pb::WaitDerivedValueRequest) -> pb::WaitDerivedValueResponse;
    }
}
