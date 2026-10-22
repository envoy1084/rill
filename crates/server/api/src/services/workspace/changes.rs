//! Snapshots, turning them into change versions, moving the base and
//! diffing the working copy.

use rill_core::{diff_trees, rebase};
use rill_proto::convert::{hash_to_proto, id_to_proto, opt_id_from_proto};
use rill_proto::rill::v1 as pb;
use rill_store::ObjectStoreExt;
use rill_types::{ChangeId, Encodable, OperationKind, Timestamp};
use tonic::Request;

use super::oplog::{base_of, base_root, commit_with_operation, operation, response_pair};
use super::WorkspaceSvc;
use crate::services::common::{self, open, operation_id, provenance, resolve_version};
use crate::services::diff::{file_diffs, DiffOptions};
use crate::services::repo::version_from_diff;
use rill_runtime::error::{Error, Result};

impl WorkspaceSvc {
    pub(super) async fn snapshot_impl(
        &self,
        request: Request<pb::SnapshotRequest>,
    ) -> Result<pb::SnapshotResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        let before = self.load_mut(&opened, &req.workspace).await?;
        let mut ws = before.clone();
        let tree = common::hash("tree_hash", &req.tree_hash)?;
        opened.repo.trees.get_tree(&tree).await.map_err(|_| {
            Error::precondition(format!("snapshot tree {tree} has not been uploaded"))
        })?;
        ws.snapshot = Some(tree);
        let op = self
            .record(
                &opened,
                Some(&before),
                &mut ws,
                operation_id(&req.operation_id)?,
                OperationKind::Snapshot { tree },
            )
            .await?;
        Ok(pb::SnapshotResponse {
            operation: Some(pb::Operation::from(op)),
        })
    }

    pub(super) async fn new_change_impl(
        &self,
        request: Request<pb::NewChangeRequest>,
    ) -> Result<pb::NewChangeResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        let before = self.load_mut(&opened, &req.workspace).await?;
        let mut ws = before.clone();
        let snapshot = ws.snapshot.ok_or_else(|| {
            Error::precondition("workspace has no snapshot to turn into a change")
        })?;
        let base_root = base_root(&opened, &ws).await?;
        let entries = diff_trees(&opened.repo.trees, base_root, Some(snapshot)).await?;
        if entries.is_empty() {
            return Err(Error::precondition(
                "snapshot is identical to the base; nothing to change",
            ));
        }
        for entry in &entries {
            opened.permit().write(&entry.path).await?;
        }
        let predecessor = opened
            .repo
            .repo
            .change_versions(&ws.working_change)
            .await?
            .last()
            .copied();
        let cv = version_from_diff(
            entries,
            ws.working_change,
            base_of(&ws).into_iter().collect(),
            predecessor,
            opened.principal().clone(),
            req.message,
            provenance(req.provenance)?,
            opt_id_from_proto("intent", req.intent.as_ref())?,
        );
        let change = cv.change_id;
        let base = ws.base;
        ws.base = cv.hash();
        ws.working_change = ChangeId::new();
        let op = operation(
            &opened,
            Some(&before),
            &mut ws,
            operation_id(&req.operation_id)?,
            OperationKind::NewChange { change, base },
        );
        let (hash, cv) = commit_with_operation(&opened, cv, &before, &op).await?;
        Ok(pb::NewChangeResponse {
            change_id: id_to_proto(&change),
            version_hash: hash_to_proto(&hash),
            version: Some(pb::ChangeVersion::from(cv)),
            operation: Some(pb::Operation::from(op)),
        })
    }

    pub(super) async fn update_base_impl(
        &self,
        request: Request<pb::UpdateBaseRequest>,
    ) -> Result<pb::UpdateBaseResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.into_inner();
        let before = self.load_mut(&opened, &req.workspace).await?;
        let mut ws = before.clone();
        let new_base = resolve_version(&opened.repo, req.new_base.as_ref())
            .await?
            .ok_or_else(|| Error::precondition("the repository has no trunk head yet"))?;
        let from = ws.base;
        let current = opened
            .repo
            .repo
            .change_versions(&ws.working_change)
            .await?
            .last()
            .copied();
        ws.base = new_base;
        let op_id = operation_id(&req.operation_id)?;
        let kind = OperationKind::Rebase { from, to: new_base };
        let (op, version, version_hash) = match current {
            Some(current) if new_base != from => {
                let rebased =
                    rebase(&opened.repo.derive, &current, &new_base, Timestamp::now()).await?;
                // The snapshot is the rebased version's tree, which
                // needs the version object stored first.
                let hash = opened.repo.repo.objects().put_object(&rebased).await?;
                ws.snapshot = Some(common::root_of(&opened.repo, &hash).await?);
                let op = operation(&opened, Some(&before), &mut ws, op_id, kind);
                let (hash, cv) = commit_with_operation(&opened, rebased, &before, &op).await?;
                (
                    op,
                    Some(pb::ChangeVersion::from(cv)),
                    Some(hash_to_proto(&hash)),
                )
            }
            _ => (
                self.record(&opened, Some(&before), &mut ws, op_id, kind)
                    .await?,
                None,
                None,
            ),
        };
        let (workspace, operation) = response_pair(&ws, &op);
        Ok(pb::UpdateBaseResponse {
            workspace,
            operation,
            version,
            version_hash,
        })
    }

    pub(super) async fn workspace_diff_impl(
        &self,
        request: Request<pb::WorkspaceDiffRequest>,
    ) -> Result<pb::WorkspaceDiffResponse> {
        let opened = open(&self.state, &request, &request.get_ref().repo).await?;
        let req = request.get_ref();
        let ws = self.load(&opened, &req.workspace).await?;
        let filter = opened.permit().read_filter().await?;
        let options = DiffOptions::new(&req.paths, req.format, req.context_lines, 0, filter)?;
        let base_root = base_root(&opened, &ws).await?;
        let (files, _) = file_diffs(&opened.repo, base_root, ws.snapshot, None, &options).await?;
        Ok(pb::WorkspaceDiffResponse { files })
    }
}
