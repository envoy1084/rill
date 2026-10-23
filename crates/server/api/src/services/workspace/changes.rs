//! Snapshots, turning them into change versions, moving the base and
//! diffing the working copy.

use pb::snapshot_workspace_request::Source;
use rill_core::tree::Edit as TreeEdit;
use rill_core::TreeBuilder;
use rill_core::{diff_trees, rebase};
use rill_proto::rill::v1 as pb;
use rill_store::ObjectStoreExt;
use rill_types::{ChangeId, Encodable, OperationKind, Timestamp, TreeHash};

use super::oplog::{
    base_of, base_root, check_etag, commit_with_operation, operation, operation_proto,
    response_pair,
};
use super::WorkspaceSvc;
use crate::services::changes::{stored_content, version_from_diff};
use crate::services::common::{self, open_repo, path, provenance, Opened};
use crate::services::diff::{file_diffs, DiffOptions};
use crate::services::{names, Call};
use rill_runtime::error::{Error, Result};

impl WorkspaceSvc {
    pub(super) async fn snapshot_workspace_impl(
        &self,
        request: Call<pb::SnapshotWorkspaceRequest>,
    ) -> Result<pb::SnapshotWorkspaceResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        let req = request.into_inner();
        let before = self.load_mut(&opened, &req.name).await?;
        check_etag(&before, &req.etag)?;
        let mut ws = before.clone();
        let tree = match req.source {
            Some(Source::TreeHash(hash)) => {
                let tree = common::hash("tree_hash", &hash)?;
                opened.repo.trees.get_tree(&tree).await.map_err(|_| {
                    Error::MissingObjects(format!("snapshot tree {tree} has not been uploaded"))
                })?;
                tree
            }
            Some(Source::Edits(edits)) => {
                let base = base_root(&opened, &ws).await?;
                let start = if edits.from_base {
                    base
                } else {
                    ws.snapshot.or(base)
                };
                build_snapshot(&opened, edits.edits, start).await?
            }
            None => return Err(Error::invalid("a snapshot needs a tree hash or edits")),
        };
        ws.snapshot = Some(tree);
        let op = self
            .record(
                &opened,
                Some(&before),
                &mut ws,
                names::request_id(&req.request_id)?,
                OperationKind::Snapshot { tree },
            )
            .await?;
        Ok(pb::SnapshotWorkspaceResponse {
            operation: operation_proto(&opened.repo, &op).into(),
        })
    }

    pub(super) async fn start_change_impl(
        &self,
        request: Call<pb::StartChangeRequest>,
    ) -> Result<pb::StartChangeResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        let req = request.into_inner();
        let before = self.load_mut(&opened, &req.name).await?;
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
            provenance(req.provenance.into_option())?,
            names::intent(&req.intent)?,
        );
        let change = cv.change_id;
        let base = ws.base;
        ws.base = cv.hash();
        ws.working_change = ChangeId::new();
        let op = operation(
            &opened,
            Some(&before),
            &mut ws,
            names::request_id(&req.request_id)?,
            OperationKind::NewChange { change, base },
        );
        let (hash, cv) = commit_with_operation(&opened, cv, &before, &op).await?;
        Ok(pb::StartChangeResponse {
            change: names::change(&opened.repo, &change),
            info: common::change_info(&opened.repo, &hash).await?.into(),
            version: pb::ChangeVersion::from(cv).into(),
            operation: operation_proto(&opened.repo, &op).into(),
        })
    }

    pub(super) async fn sync_workspace_impl(
        &self,
        request: Call<pb::SyncWorkspaceRequest>,
    ) -> Result<pb::SyncWorkspaceResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        let req = request.into_inner();
        let before = self.load_mut(&opened, &req.name).await?;
        let mut ws = before.clone();
        let new_base = crate::services::revision::resolve(&opened.repo, &req.base)
            .await?
            .version
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
        let op_id = names::request_id(&req.request_id)?;
        let kind = OperationKind::Rebase { from, to: new_base };
        let (op, version, info) = match current {
            Some(current) if new_base != from => {
                let rebased =
                    rebase(&opened.repo.derive, &current, &new_base, Timestamp::now()).await?;
                // The snapshot is the rebased version's tree, which
                // needs the version object stored first.
                let hash = opened.repo.repo.objects().put_object(&rebased).await?;
                ws.snapshot = Some(common::root_of(&opened.repo, &hash).await?);
                let op = operation(&opened, Some(&before), &mut ws, op_id, kind);
                let (hash, cv) = commit_with_operation(&opened, rebased, &before, &op).await?;
                let info = common::change_info(&opened.repo, &hash).await?;
                (op, Some(pb::ChangeVersion::from(cv)), Some(info))
            }
            _ => (
                self.record(&opened, Some(&before), &mut ws, op_id, kind)
                    .await?,
                None,
                None,
            ),
        };
        let (workspace, operation) = response_pair(&opened.repo, &ws, &op);
        Ok(pb::SyncWorkspaceResponse {
            workspace: workspace.into(),
            operation: operation.into(),
            info: info.into(),
            version: version.into(),
        })
    }

    pub(super) async fn diff_workspace_impl(
        &self,
        request: Call<pb::DiffWorkspaceRequest>,
    ) -> Result<pb::DiffWorkspaceResponse> {
        let opened = open_repo(&self.state, &request, &request.get_ref().name).await?;
        let req = request.get_ref();
        let ws = self.load(&opened, &req.name).await?;
        let filter = opened.permit().read_filter().await?;
        let options = DiffOptions::new(&req.paths, req.format, req.context_lines, 0, filter)?;
        let base_root = base_root(&opened, &ws).await?;
        let (files, _) = file_diffs(&opened.repo, base_root, ws.snapshot, None, &options).await?;
        Ok(pb::DiffWorkspaceResponse { files })
    }
}

/// Apply snapshot edits to `start` (the empty tree when `None`).
async fn build_snapshot(
    opened: &Opened,
    edits: Vec<pb::Edit>,
    start: Option<TreeHash>,
) -> Result<TreeHash> {
    if edits.len() > rill_types::limits::MAX_FILES_PER_CHANGE {
        return Err(Error::invalid(format!(
            "{} edits; the limit is {}",
            edits.len(),
            rill_types::limits::MAX_FILES_PER_CHANGE
        )));
    }
    let objects = opened.repo.repo.objects();
    let mut builder = TreeBuilder::new();
    for edit in edits {
        let path = path("edits.path", &edit.path)?;
        if path.is_root() {
            return Err(Error::invalid("an edit cannot target the repository root"));
        }
        opened.permit().write(&path).await?;
        let edit = match edit.op {
            Some(pb::edit::Op::SetContent(set)) => {
                let (content, kind) = stored_content(objects.as_ref(), set).await?;
                TreeEdit::Set(content, kind)
            }
            Some(pb::edit::Op::Delete(_)) => TreeEdit::Delete,
            Some(pb::edit::Op::Patch(_)) => {
                return Err(Error::invalid(format!(
                    "{path:?}: snapshots take content, not patches"
                )))
            }
            None => return Err(Error::invalid(format!("edit for {path:?} has no op"))),
        };
        builder.edit(&path, edit);
    }
    Ok(builder.build(&opened.repo.trees, start).await?)
}
