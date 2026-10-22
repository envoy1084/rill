//! Workspace RPC results.

use rill_proto::convert::required;
use rill_proto::rill::v1 as pb;
use rill_types::{
    ChangeId, ChangeInfo, ChangeVersion, ChangeVersionHash, IntentId, Operation, Provenance,
    Workspace,
};

use crate::error::{Error, Result};

/// A workspace after a mutation, with the operation that mutated it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceOp {
    /// The workspace.
    pub workspace: Workspace,
    /// The operation appended to its log.
    pub operation: Operation,
}

impl WorkspaceOp {
    pub(crate) fn from_parts(
        workspace: Option<pb::Workspace>,
        operation: Option<pb::WorkspaceOperation>,
    ) -> Result<Self> {
        Ok(WorkspaceOp {
            workspace: required("response.workspace", workspace)?.try_into()?,
            operation: required("response.operation", operation)?.try_into()?,
        })
    }
}

/// Filters for listing workspaces.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WorkspaceFilter {
    /// Only workspaces owned by this principal, as shown (`ann`).
    pub owner: Option<String>,
    /// Page size; `0` lets the server choose.
    pub limit: u32,
    /// Continue from a previous page.
    pub page_token: Option<String>,
}

/// Metadata for a change created from a workspace snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewChange {
    /// Change message.
    pub message: String,
    /// Intent the change belongs to.
    pub intent: Option<IntentId>,
    /// Who and what produced it.
    pub provenance: Provenance,
}

impl NewChange {
    /// A change with only a message.
    pub fn message(message: impl Into<String>) -> Self {
        NewChange {
            message: message.into(),
            ..Default::default()
        }
    }
}

/// A change created from a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewChangeResult {
    /// Stable identity of the new change.
    pub change_id: ChangeId,
    /// Hash of its first version.
    pub hash: ChangeVersionHash,
    /// The version.
    pub version: ChangeVersion,
    /// The workspace operation.
    pub operation: Operation,
}

impl TryFrom<pb::StartChangeResponse> for NewChangeResult {
    type Error = Error;

    fn try_from(r: pb::StartChangeResponse) -> Result<Self> {
        let info: ChangeInfo = required("start_change.info", r.info)?.try_into()?;
        Ok(NewChangeResult {
            change_id: info.change_id,
            hash: info.hash,
            version: required("start_change.version", r.version)?.try_into()?,
            operation: required("start_change.operation", r.operation)?.try_into()?,
        })
    }
}

/// Result of moving a workspace's base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateBaseResult {
    /// The workspace.
    pub workspace: Workspace,
    /// The operation.
    pub operation: Operation,
    /// The rebased working change, if one existed.
    pub rebased: Option<(ChangeVersionHash, ChangeVersion)>,
}

impl TryFrom<pb::SyncWorkspaceResponse> for UpdateBaseResult {
    type Error = Error;

    fn try_from(r: pb::SyncWorkspaceResponse) -> Result<Self> {
        let rebased = match (r.info.into_option(), r.version.into_option()) {
            (Some(info), Some(v)) => {
                let info: ChangeInfo = info.try_into()?;
                Some((info.hash, v.try_into()?))
            }
            _ => None,
        };
        Ok(UpdateBaseResult {
            workspace: required("sync_workspace.workspace", r.workspace)?.try_into()?,
            operation: required("sync_workspace.operation", r.operation)?.try_into()?,
            rebased,
        })
    }
}
