//! The operation log: listing it and undoing an operation.

use rill_proto::convert::opt_id_from_proto;
use rill_proto::rill::v1 as pb;
use rill_types::{OperationId, OperationKind, Workspace};

use super::oplog::response_pair;
use super::WorkspaceSvc;
use crate::services::common::{self, open, operation_id};
use crate::services::Call;
use rill_runtime::error::{Error, Result};

/// Default page size for operation listings.
const DEFAULT_OPS_LIMIT: usize = 50;
const MAX_OPS_LIMIT: usize = 1000;

impl WorkspaceSvc {
    pub(super) async fn undo_impl(
        &self,
        request: Call<pb::UndoRequest>,
    ) -> Result<pb::UndoResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        let before = self.load_mut(&opened, &req.workspace).await?;
        let mut ws = before.clone();
        let ops = opened
            .repo
            .repo
            .list_operations(&ws.id, MAX_OPS_LIMIT)
            .await?;
        let target: OperationId = match opt_id_from_proto("target", req.target.as_ref())? {
            Some(id) => id,
            None => ws
                .head_op
                .ok_or_else(|| Error::precondition("workspace has no operations"))?,
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
                operation_id(&req.operation_id)?,
                OperationKind::Undo { undone: target },
            )
            .await?;
        let (workspace, operation) = response_pair(&ws, &op);
        Ok(pb::UndoResponse {
            workspace: workspace.into(),
            operation: operation.into(),
        })
    }

    pub(super) async fn list_operations_impl(
        &self,
        request: Call<pb::ListOperationsRequest>,
    ) -> Result<pb::ListOperationsResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.get_ref();
        let ws = self.load(&opened, &req.workspace).await?;
        let limit = common::limit(req.limit, DEFAULT_OPS_LIMIT, MAX_OPS_LIMIT);
        let before: Option<OperationId> = opt_id_from_proto("before", req.before.as_ref())?;
        let mut ops = opened
            .repo
            .repo
            .list_operations(&ws.id, MAX_OPS_LIMIT)
            .await?;
        if let Some(before) = before {
            if let Some(pos) = ops.iter().position(|o| o.id == before) {
                ops.truncate(pos);
            }
        }
        let start = ops.len().saturating_sub(limit);
        let operations = ops.drain(start..).map(pb::Operation::from).collect();
        Ok(pb::ListOperationsResponse { operations })
    }
}
