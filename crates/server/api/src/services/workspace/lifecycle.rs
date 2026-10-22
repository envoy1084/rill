//! Creating, reading, listing, reconfiguring, handing off and deleting
//! workspaces.

use rill_proto::convert::opt_id_from_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{
    ChangeId, ObjectHash, OperationKind, Principal, RepoPathBuf, SparseProfile, Timestamp,
    Workspace, WorkspaceId,
};
use tonic::Request;

use super::oplog::response_pair;
use super::WorkspaceSvc;
use crate::services::common::{self, open, operation_id, resolve_version};
use rill_runtime::error::{Error, Result};

impl WorkspaceSvc {
    pub(super) async fn create_workspace_impl(
        &self,
        request: Request<pb::CreateWorkspaceRequest>,
    ) -> Result<pb::CreateWorkspaceResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        opened.permit().write(&RepoPathBuf::root()).await?;
        let base = resolve_version(&opened.repo, req.base.as_ref())
            .await?
            .unwrap_or(ObjectHash::ZERO);
        let sparse = match req.sparse {
            Some(s) => SparseProfile::try_from(s)?,
            None => SparseProfile::full(),
        };
        let now = Timestamp::now();
        let mut ws = Workspace {
            id: WorkspaceId::new(),
            repo: opened.repo.id(),
            owner: opened.principal().clone(),
            name: req.name,
            base,
            working_change: ChangeId::new(),
            snapshot: None,
            sparse,
            head_op: None,
            created_at: now,
            updated_at: now,
        };
        let op = self
            .record(
                &opened,
                None,
                &mut ws,
                operation_id(&req.operation_id)?,
                OperationKind::Create,
            )
            .await?;
        let (workspace, operation) = response_pair(&ws, &op);
        Ok(pb::CreateWorkspaceResponse {
            workspace,
            operation,
        })
    }

    pub(super) async fn get_workspace_impl(
        &self,
        request: Request<pb::GetWorkspaceRequest>,
    ) -> Result<pb::GetWorkspaceResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let ws = self.load(&opened, &request.get_ref().workspace).await?;
        Ok(pb::GetWorkspaceResponse {
            workspace: Some(pb::Workspace::from(ws)),
        })
    }

    pub(super) async fn list_workspaces_impl(
        &self,
        request: Request<pb::ListWorkspacesRequest>,
    ) -> Result<pb::ListWorkspacesResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let req = request.get_ref();
        let owner: Option<rill_types::PrincipalId> =
            opt_id_from_proto("owner", req.owner.as_ref())?;
        let limit = common::limit(req.limit, 100, 10_000);
        let workspaces = opened
            .repo
            .repo
            .list_workspaces()
            .await?
            .into_iter()
            .filter(|w| owner.is_none_or(|o| w.owner.id == o))
            .take(limit)
            .map(pb::Workspace::from)
            .collect();
        Ok(pb::ListWorkspacesResponse {
            workspaces,
            next_page_token: None,
        })
    }

    pub(super) async fn set_sparse_profile_impl(
        &self,
        request: Request<pb::SetSparseProfileRequest>,
    ) -> Result<pb::SetSparseProfileResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        let before = self.load_mut(&opened, &req.workspace).await?;
        let mut ws = before.clone();
        let profile = match req.sparse {
            Some(s) => SparseProfile::try_from(s)?,
            None => SparseProfile::full(),
        };
        ws.sparse = profile.clone();
        let op = self
            .record(
                &opened,
                Some(&before),
                &mut ws,
                operation_id(&req.operation_id)?,
                OperationKind::SetSparse { profile },
            )
            .await?;
        let (workspace, operation) = response_pair(&ws, &op);
        Ok(pb::SetSparseProfileResponse {
            workspace,
            operation,
        })
    }

    pub(super) async fn handoff_impl(
        &self,
        request: Request<pb::HandoffRequest>,
    ) -> Result<pb::HandoffResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        let before = self.load_mut(&opened, &req.workspace).await?;
        let mut ws = before.clone();
        let to = Principal::try_from(req.to.ok_or_else(|| Error::invalid("to is required"))?)?;
        ws.owner = to.clone();
        let op = self
            .record(
                &opened,
                Some(&before),
                &mut ws,
                operation_id(&req.operation_id)?,
                OperationKind::Handoff { to },
            )
            .await?;
        let (workspace, operation) = response_pair(&ws, &op);
        Ok(pb::HandoffResponse {
            workspace,
            operation,
        })
    }

    pub(super) async fn delete_workspace_impl(
        &self,
        request: Request<pb::DeleteWorkspaceRequest>,
    ) -> Result<pb::DeleteWorkspaceResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let ws = self.load_mut(&opened, &request.get_ref().workspace).await?;
        opened
            .repo
            .repo
            .delete_workspace(&ws.id, opened.principal())
            .await?;
        Ok(pb::DeleteWorkspaceResponse {})
    }
}
