//! The operation log: listing it and undoing an operation.

use rill_proto::rill::v1 as pb;
use rill_types::{OperationId, OperationKind, Workspace};

use super::oplog::{operation_proto, response_pair, workspace_id};
use super::WorkspaceSvc;
use crate::listing::{Page, Position};
use crate::services::common::open_repo;
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};

/// The most of a workspace's log that is read at once.
const MAX_OPS_LIMIT: usize = 1000;

impl WorkspaceSvc {
    pub(super) async fn undo_operation_impl(
        &self,
        request: Call<pb::UndoOperationRequest>,
    ) -> Result<pb::UndoOperationResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        let req = request.into_inner();
        let (workspace, operation) = req.name.rsplit_once("/operations/").ok_or_else(|| {
            Error::InvalidName(format!("{:?} is not an operation name", req.name))
        })?;
        workspace_id(workspace)?;
        let before = self.load_mut(&opened, workspace).await?;
        let mut ws = before.clone();
        let ops = opened
            .repo
            .repo
            .list_operations(&ws.id, MAX_OPS_LIMIT)
            .await?;
        let target: OperationId = match operation {
            "head" => ws
                .head_op
                .ok_or_else(|| Error::precondition("workspace has no operations"))?,
            id => id
                .parse()
                .map_err(|_| Error::InvalidName(format!("{id:?} is not an operation id")))?,
        };
        let undone = ops
            .iter()
            .find(|o| o.id == target)
            .ok_or_else(|| Error::not_found(format!("operation {target}")))?;
        let parent_id = undone.parent.ok_or_else(|| {
            Error::precondition("the workspace's first operation cannot be undone")
        })?;
        let previous = ops.iter().find(|o| o.id == parent_id).ok_or_else(|| {
            Error::not_found(format!("operation {parent_id} is beyond the retained log"))
        })?;
        // Everything but identity comes back: base, snapshot, working
        // change, sparse profile and owner.
        ws = Workspace {
            id: ws.id,
            repo: ws.repo,
            created_at: ws.created_at,
            head_op: ws.head_op,
            updated_at: ws.updated_at,
            ..previous.resulting.clone()
        };
        let op = self
            .record(
                &opened,
                Some(&before),
                &mut ws,
                names::request_id(&req.request_id)?,
                OperationKind::Undo { undone: target },
            )
            .await?;
        let (workspace, operation) = response_pair(&opened.repo, &ws, &op);
        Ok(pb::UndoOperationResponse {
            workspace: workspace.into(),
            operation: operation.into(),
        })
    }

    pub(super) async fn list_workspace_operations_impl(
        &self,
        request: Call<pb::ListWorkspaceOperationsRequest>,
    ) -> Result<pb::ListWorkspaceOperationsResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().parent).await?;
        let req = request.get_ref();
        let ws = self.load(&opened, &req.parent).await?;
        let page = Page::new(req.page_size, &req.page_token, &[&req.parent])?;
        // The log, newest first; a page continues after the operation the
        // previous one ended with.
        let mut ops = opened
            .repo
            .repo
            .list_operations(&ws.id, MAX_OPS_LIMIT)
            .await?;
        ops.reverse();
        let skip = match &page.start {
            Position::Start => 0,
            Position::After(key) => {
                let after =
                    OperationId::from_bytes(key.as_slice().try_into().map_err(|_| {
                        Error::InvalidPageToken("not a token for this list".into())
                    })?);
                ops.iter()
                    .position(|o| o.id == after)
                    .map_or(ops.len(), |i| i + 1)
            }
            Position::Offset(_) => {
                return Err(Error::InvalidPageToken("not a token for this list".into()))
            }
        };
        let rest = &ops[skip.min(ops.len())..];
        let taken = &rest[..page.size.min(rest.len())];
        let next_page_token = match (taken.last(), rest.len() > taken.len()) {
            (Some(last), true) => page.token(Position::After(last.id.as_bytes().to_vec())),
            _ => String::new(),
        };
        Ok(pb::ListWorkspaceOperationsResponse {
            operations: taken
                .iter()
                .map(|op| operation_proto(&opened.repo, op))
                .collect(),
            next_page_token,
        })
    }
}
