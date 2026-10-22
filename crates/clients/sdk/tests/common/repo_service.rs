//! `RepoService` implementation backed by [`MockRepo`].

use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest, ServiceStream};
use rill_proto::convert::{hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::RepoService;
use rill_types::{ChangeId, Principal};

use super::fixtures::fake_hash;
use super::mock_repo::MockRepo;
use super::rpc::{unimplemented_rpcs, Request, RpcResult};

fn landing(position: u64, hash: &rill_types::ObjectHash) -> pb::Landing {
    pb::Landing {
        name: format!("orgs/default/repos/main/trunk/positions/{position}"),
        position,
        version: String::new(),
        version_hash: hash_to_proto(hash),
        change_id: id_to_proto(&ChangeId::derived("land", &position.to_string())),
        landed_by: (pb::Principal::from(Principal::service("lander"))).into(),
        landed_at: position as i64,
        reverts_position: None,
    }
}

// The mocks return concrete types.
#[allow(refining_impl_trait)]
impl RepoService for MockRepo {
    async fn get_repo(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::GetRepoRequest>,
    ) -> RpcResult<pb::GetRepoResponse> {
        let req = Request::new(ctx, req);
        self.record_auth(&req);
        let name = req.into_inner().name;
        let short = name.rsplit('/').next().unwrap_or_default().to_string();
        if short == "missing" {
            return Err(ConnectError::not_found("no such repo"));
        }
        Ok(Response::new(pb::GetRepoResponse {
            repo: (pb::Repo {
                uid: id_to_proto(&rill_types::RepoId::derived("test", &short)),
                name,
                repo_id: short,
                description: Some("mock".into()),
                created_at: 1,
                trunk_head_position: self.trunk.lock().unwrap().0,
                windows_safe: false,
                org: "default".into(),
            })
            .into(),
        }))
    }

    async fn resolve_revision(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ResolveRevisionRequest>,
    ) -> RpcResult<pb::ResolveRevisionResponse> {
        let revision = Request::new(ctx, req).into_inner().revision;
        let (seq, hash) = *self.trunk.lock().unwrap();
        match revision.as_str() {
            "" | "trunk" => Ok(Response::new(pb::ResolveRevisionResponse {
                version: String::new(),
                version_hash: hash_to_proto(&hash),
                tree_hash: Default::default(),
                trunk_position: Some(seq),
            })),
            other => Err(ConnectError::not_found(format!("revision {other}"))),
        }
    }

    async fn get_trunk_position(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::GetTrunkPositionRequest>,
    ) -> RpcResult<pb::GetTrunkPositionResponse> {
        let req = Request::new(ctx, req);
        self.record_auth(&req);
        let (seq, hash) = *self.trunk.lock().unwrap();
        Ok(Response::new(pb::GetTrunkPositionResponse {
            landing: landing(seq, &hash).into(),
        }))
    }

    async fn watch_trunk(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::WatchTrunkRequest>,
    ) -> RpcResult<ServiceStream<pb::WatchTrunkResponse>> {
        let _req = Request::new(ctx, req);
        let events = (1..=3u64).map(|seq| {
            Ok(pb::WatchTrunkResponse {
                landing: landing(seq, &fake_hash(&format!("land{seq}"))).into(),
            })
        });
        Ok(Response::new(Box::pin(tokio_stream::iter(events))))
    }

    unimplemented_rpcs! {
        list_repos(pb::ListReposRequest) -> pb::ListReposResponse;
        create_repo(pb::CreateRepoRequest) -> pb::CreateRepoResponse;
        get_ref(pb::GetRefRequest) -> pb::GetRefResponse;
        list_refs(pb::ListRefsRequest) -> pb::ListRefsResponse;
        update_ref(pb::UpdateRefRequest) -> pb::UpdateRefResponse;
        delete_ref(pb::DeleteRefRequest) -> pb::DeleteRefResponse;
        create_release(pb::CreateReleaseRequest) -> pb::CreateReleaseResponse;
        list_trunk_positions(pb::ListTrunkPositionsRequest) -> pb::ListTrunkPositionsResponse;
        watch_refs(pb::WatchRefsRequest) -> ServiceStream<pb::WatchRefsResponse>;
        watch_versions(pb::WatchVersionsRequest) -> ServiceStream<pb::WatchVersionsResponse>;
        get_derived_value(pb::GetDerivedValueRequest) -> pb::GetDerivedValueResponse;
        wait_derived_value(pb::WaitDerivedValueRequest) -> pb::WaitDerivedValueResponse;
    }
}
