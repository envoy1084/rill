//! `ChangeService` implementation backed by [`MockRepo`].

use std::collections::BTreeMap;

use bytes::Bytes;
use connectrpc::{ConnectError, RequestContext, Response, ServiceRequest};
use rill_proto::convert::{hash_to_proto, id_to_proto};
use rill_proto::rill::v1 as pb;
use rill_proto::rill::v1::ChangeService;
use rill_types::{ChangeId, ObjectHash, Principal};

use super::fixtures::{fake_hash, sample_info};
use super::mock_repo::MockRepo;
use super::rpc::{unimplemented_rpcs, Request, RpcResult};

/// The id of the change a `…/changes/{change}` name names.
fn change_of(name: &str) -> Result<ChangeId, ConnectError> {
    name.rsplit('/')
        .next()
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| ConnectError::invalid_argument(format!("bad change name {name:?}")))
}

/// The info a version built here reports.
fn info(hash: &ObjectHash, version: &pb::ChangeVersion) -> pb::ChangeInfo {
    pb::ChangeInfo {
        hash: hash_to_proto(hash),
        change_id: version.change_id.clone(),
        parents: version.parents.clone(),
        message: version.message.clone(),
        author: version.author.clone(),
        created_at: version.created_at,
        ..Default::default()
    }
}

// The mocks return concrete types.
#[allow(refining_impl_trait)]
impl ChangeService for MockRepo {
    async fn list_versions(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ListVersionsRequest>,
    ) -> RpcResult<pb::ListVersionsResponse> {
        let parent = Request::new(ctx, req).into_inner().parent;
        let id = change_of(&parent)?;
        Ok(Response::new(pb::ListVersionsResponse {
            versions: vec![sample_info(&id.to_string(), None)],
            next_page_token: String::new(),
        }))
    }

    async fn apply_edits(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::ApplyEditsRequest>,
    ) -> RpcResult<pb::ApplyEditsResponse> {
        let req = Request::new(ctx, req).into_inner();
        self.applied.lock().unwrap().push(req.clone());
        let failing = self.unavailable.load(std::sync::atomic::Ordering::SeqCst);
        if failing > 0 {
            self.unavailable
                .store(failing - 1, std::sync::atomic::Ordering::SeqCst);
            return Err(ConnectError::unavailable("server going away"));
        }
        let mut file_changes = BTreeMap::new();
        for edit in &req.edits {
            let change = match &edit.op {
                Some(pb::edit::Op::SetContent(set)) => {
                    pb::file_change::Change::Set(pb::file_change::Set {
                        content: set.content.clone(),
                        kind: set.kind,
                        copy_from: None.into(),
                    })
                }
                Some(pb::edit::Op::Delete(_)) => {
                    pb::file_change::Change::Delete(pb::file_change::Delete {})
                }
                Some(pb::edit::Op::Patch(p)) => {
                    pb::file_change::Change::Set(pb::file_change::Set {
                        content: (pb::ContentRef {
                            r#ref: Some(pb::content_ref::Ref::Inline(Bytes::from(p.clone()))),
                        })
                        .into(),
                        kind: pb::FileKind::Regular.into(),
                        copy_from: None.into(),
                    })
                }
                None => return Err(ConnectError::invalid_argument("edit without op")),
            };
            file_changes.insert(
                edit.path.clone(),
                pb::FileChange {
                    change: Some(change),
                },
            );
        }
        let change_id = match req.change.as_str() {
            "" => ChangeId::new(),
            name => change_of(name)?,
        };
        let version = pb::ChangeVersion {
            change_id: id_to_proto(&change_id),
            parents: vec![hash_to_proto(&self.trunk.lock().unwrap().1)],
            predecessor: None,
            subtree_changes: BTreeMap::new(),
            file_changes,
            author: (pb::Principal::from(Principal::human("alice"))).into(),
            provenance: req.provenance.clone(),
            intent: None,
            message: req.message.clone(),
            created_at: 42,
            extra: BTreeMap::new(),
        };
        let hash = fake_hash(&format!("apply:{}:{}", req.message, req.edits.len()));
        Ok(Response::new(pb::ApplyEditsResponse {
            info: info(&hash, &version).into(),
            version: version.into(),
        }))
    }

    async fn rebase_version(
        &self,
        ctx: RequestContext,
        req: ServiceRequest<'_, pb::RebaseVersionRequest>,
    ) -> RpcResult<pb::RebaseVersionResponse> {
        let req = Request::new(ctx, req).into_inner();
        let parse = |text: &str| {
            text.parse::<ObjectHash>()
                .map_err(|_| ConnectError::invalid_argument(format!("bad hash {text:?}")))
        };
        let from = parse(req.name.rsplit('/').next().unwrap_or_default())?;
        let onto = parse(&req.onto)?;
        self.rebases.lock().unwrap().push((from, onto));
        let new_hash = fake_hash(&format!("rebase:{from}:{onto}"));
        let version = pb::ChangeVersion {
            change_id: id_to_proto(&ChangeId::derived("rebase", &from.to_hex())),
            parents: vec![hash_to_proto(&onto)],
            predecessor: Some(hash_to_proto(&from)),
            subtree_changes: BTreeMap::new(),
            file_changes: BTreeMap::new(),
            author: (pb::Principal::from(Principal::human("alice"))).into(),
            provenance: None.into(),
            intent: None,
            message: "rebased".into(),
            created_at: 1,
            extra: BTreeMap::new(),
        };
        Ok(Response::new(pb::RebaseVersionResponse {
            info: info(&new_hash, &version).into(),
            version: version.into(),
        }))
    }

    unimplemented_rpcs! {
        get_version(pb::GetVersionRequest) -> pb::GetVersionResponse;
        batch_get_versions(pb::BatchGetVersionsRequest) -> pb::BatchGetVersionsResponse;
        create_version(pb::CreateVersionRequest) -> pb::CreateVersionResponse;
        merge_versions(pb::MergeVersionsRequest) -> pb::MergeVersionsResponse;
        resolve_conflicts(pb::ResolveConflictsRequest) -> pb::ResolveConflictsResponse;
    }
}
