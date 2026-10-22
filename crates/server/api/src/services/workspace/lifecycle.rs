//! Creating, reading, listing, reconfiguring, handing off and deleting
//! workspaces.

use rill_proto::rill::v1 as pb;
use rill_types::{
    ChangeId, ObjectHash, OperationKind, Principal, RepoPathBuf, SparseProfile, Timestamp,
    Workspace, WorkspaceId,
};

use super::oplog::{check_etag, response_pair, workspace_proto};
use super::WorkspaceSvc;
use crate::listing::{FieldValue, Filter, Kind, Page, Record};
use crate::services::common::open_repo;
use crate::services::{names, Call};
use rill_runtime::error::Result;

impl WorkspaceSvc {
    pub(super) async fn create_workspace_impl(
        &self,
        request: Call<pb::CreateWorkspaceRequest>,
    ) -> Result<pb::CreateWorkspaceResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        let req = request.into_inner();
        opened.permit().write(&RepoPathBuf::root()).await?;
        let given = req.workspace.into_option().unwrap_or_default();
        let base = crate::services::revision::resolve(&opened.repo, &req.base)
            .await?
            .version
            .unwrap_or(ObjectHash::ZERO);
        let sparse = match given.sparse.into_option() {
            Some(s) => SparseProfile::try_from(s)?,
            None => SparseProfile::full(),
        };
        let now = Timestamp::now();
        let mut ws = Workspace {
            id: WorkspaceId::new(),
            repo: opened.repo.id(),
            owner: opened.principal().clone(),
            name: given.display_name,
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
                names::request_id(&req.request_id)?,
                OperationKind::Create,
            )
            .await?;
        let (workspace, operation) = response_pair(&opened.repo, &ws, &op);
        Ok(pb::CreateWorkspaceResponse {
            workspace: workspace.into(),
            operation: operation.into(),
        })
    }

    pub(super) async fn get_workspace_impl(
        &self,
        request: Call<pb::GetWorkspaceRequest>,
    ) -> Result<pb::GetWorkspaceResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        let ws = self.load(&opened, &request.get_ref().name).await?;
        Ok(pb::GetWorkspaceResponse {
            workspace: workspace_proto(&opened.repo, &ws).into(),
        })
    }

    pub(super) async fn list_workspaces_impl(
        &self,
        request: Call<pb::ListWorkspacesRequest>,
    ) -> Result<pb::ListWorkspacesResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        opened.permit().read(&RepoPathBuf::root()).await?;
        let req = request.get_ref();
        let filter = Filter::parse(
            &req.filter,
            &[("owner", Kind::Str), ("display_name", Kind::Str)],
        )?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent, &req.filter])?;
        let mut workspaces: Vec<Workspace> = opened
            .repo
            .repo
            .list_workspaces()
            .await?
            .into_iter()
            .filter(|w| filter.matches(&Listed(w)))
            .collect();
        workspaces.sort_by_key(|w| w.id);
        let (workspaces, next_page_token) = page.of(workspaces)?;
        Ok(pb::ListWorkspacesResponse {
            workspaces: workspaces
                .iter()
                .map(|w| workspace_proto(&opened.repo, w))
                .collect(),
            next_page_token,
        })
    }

    pub(super) async fn update_workspace_impl(
        &self,
        request: Call<pb::UpdateWorkspaceRequest>,
    ) -> Result<pb::UpdateWorkspaceResponse> {
        let req = request.get_ref();
        let given = req
            .workspace
            .as_option()
            .ok_or(rill_proto::ConvertError::missing("workspace"))?;
        let opened = open_repo(&self.state, &request, &given.name).await?;
        let req = request.into_inner();
        let given = req.workspace.into_option().unwrap_or_default();
        let before = self.load_mut(&opened, &given.name).await?;
        check_etag(&before, &req.etag)?;
        let mut ws = before.clone();
        let profile = match given.sparse.into_option() {
            Some(s) => SparseProfile::try_from(s)?,
            None => SparseProfile::full(),
        };
        ws.sparse = profile.clone();
        let op = self
            .record(
                &opened,
                Some(&before),
                &mut ws,
                names::request_id(&req.request_id)?,
                OperationKind::SetSparse { profile },
            )
            .await?;
        let (workspace, operation) = response_pair(&opened.repo, &ws, &op);
        Ok(pb::UpdateWorkspaceResponse {
            workspace: workspace.into(),
            operation: operation.into(),
        })
    }

    pub(super) async fn handoff_workspace_impl(
        &self,
        request: Call<pb::HandoffWorkspaceRequest>,
    ) -> Result<pb::HandoffWorkspaceResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        let req = request.into_inner();
        let before = self.load_mut(&opened, &req.name).await?;
        let mut ws = before.clone();
        let to = Principal::try_from(
            req.to
                .into_option()
                .ok_or(rill_proto::ConvertError::missing("to"))?,
        )?;
        ws.owner = to.clone();
        let op = self
            .record(
                &opened,
                Some(&before),
                &mut ws,
                names::request_id(&req.request_id)?,
                OperationKind::Handoff { to },
            )
            .await?;
        let (workspace, operation) = response_pair(&opened.repo, &ws, &op);
        Ok(pb::HandoffWorkspaceResponse {
            workspace: workspace.into(),
            operation: operation.into(),
        })
    }

    pub(super) async fn delete_workspace_impl(
        &self,
        request: Call<pb::DeleteWorkspaceRequest>,
    ) -> Result<pb::DeleteWorkspaceResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        let ws = self.load_mut(&opened, &request.get_ref().name).await?;
        check_etag(&ws, &request.get_ref().etag)?;
        opened
            .repo
            .repo
            .delete_workspace(&ws.id, opened.principal())
            .await?;
        Ok(pb::DeleteWorkspaceResponse {})
    }
}

/// A workspace as a list filter sees it.
struct Listed<'a>(&'a Workspace);

impl Record for Listed<'_> {
    fn field(&self, field: &str) -> FieldValue {
        match field {
            "owner" => FieldValue::Str(self.0.owner.to_string()),
            "display_name" => FieldValue::Str(self.0.name.clone()),
            _ => FieldValue::Missing,
        }
    }
}
