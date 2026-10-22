//! Workspaces and their operation log.

use rill_sdk::{NewChangeResult, UpdateBaseResult, WorkspaceOp};
use rill_types::{Operation, OperationKind, SparseProfile, Workspace};
use serde_json::{json, Value};

use super::history::change_version;
use super::primitives::{hash, opt, principal, timestamp};

/// A sparse profile.
pub fn sparse_profile(s: &SparseProfile) -> Value {
    json!({
        "include": s.include.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        "exclude": s.exclude.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
    })
}

/// A workspace record.
pub fn workspace(w: &Workspace) -> Value {
    json!({
        "id": w.id.to_string(),
        "repo": w.repo.to_string(),
        "owner": principal(&w.owner),
        "name": w.name,
        "base": hash(&w.base),
        "working_change": w.working_change.to_string(),
        "snapshot": opt(w.snapshot.as_ref(), hash),
        "sparse": sparse_profile(&w.sparse),
        "head_op": w.head_op.map(|o| o.to_string()),
        "created_at": timestamp(&w.created_at),
        "updated_at": timestamp(&w.updated_at),
    })
}

fn operation_kind(k: &OperationKind) -> Value {
    match k {
        OperationKind::Create => json!({ "kind": "create" }),
        OperationKind::Snapshot { tree } => json!({ "kind": "snapshot", "tree": hash(tree) }),
        OperationKind::NewChange { change, base } => {
            json!({ "kind": "new_change", "change": change.to_string(), "base": hash(base) })
        }
        OperationKind::Describe { version } => {
            json!({ "kind": "describe", "version": hash(version) })
        }
        OperationKind::Rebase { from, to } => {
            json!({ "kind": "rebase", "from": hash(from), "to": hash(to) })
        }
        OperationKind::Resolve { version } => {
            json!({ "kind": "resolve", "version": hash(version) })
        }
        OperationKind::SetSparse { profile } => {
            json!({ "kind": "set_sparse", "profile": sparse_profile(profile) })
        }
        OperationKind::Handoff { to } => json!({ "kind": "handoff", "to": principal(to) }),
        OperationKind::Undo { undone } => json!({ "kind": "undo", "undone": undone.to_string() }),
        OperationKind::Other { description } => {
            json!({ "kind": "other", "description": description })
        }
    }
}

/// One operation-log entry.
pub fn operation(o: &Operation) -> Value {
    json!({
        "id": o.id.to_string(),
        "workspace": o.workspace.to_string(),
        "parent": o.parent.map(|p| p.to_string()),
        "actor": principal(&o.actor),
        "op": operation_kind(&o.kind),
        "resulting": workspace(&o.resulting),
        "at": timestamp(&o.at),
    })
}

/// A workspace after a mutation.
pub fn workspace_op(w: &WorkspaceOp) -> Value {
    json!({ "workspace": workspace(&w.workspace), "operation": operation(&w.operation) })
}

/// A change created from a workspace snapshot.
pub fn new_change_result(r: &NewChangeResult) -> Value {
    json!({
        "change_id": r.change_id.to_string(),
        "hash": hash(&r.hash),
        "version": change_version(&r.version),
        "operation": operation(&r.operation),
    })
}

/// The result of moving a workspace's base.
pub fn update_base_result(r: &UpdateBaseResult) -> Value {
    json!({
        "workspace": workspace(&r.workspace),
        "operation": operation(&r.operation),
        "rebased": r.rebased.as_ref().map(|(h, v)| json!({
            "hash": hash(h),
            "version": change_version(v),
        })),
    })
}
