//! Workspaces, sparse profiles and the operation log.

use rill_types::{Operation, OperationKind, Principal, SparseProfile, Workspace};

use crate::convert::primitives::{
    hash_from_proto, hash_to_proto, id_from_proto, id_to_proto, opt_hash_from_proto,
    opt_id_from_proto, path_from_proto, required, timestamp_from_proto, timestamp_to_proto,
};
use crate::error::{ConvertError, Result};
use crate::rill::v1 as pb;

impl From<&SparseProfile> for pb::SparseProfile {
    fn from(s: &SparseProfile) -> Self {
        pb::SparseProfile {
            include: s.include.iter().map(ToString::to_string).collect(),
            exclude: s.exclude.iter().map(ToString::to_string).collect(),
        }
    }
}

impl From<SparseProfile> for pb::SparseProfile {
    fn from(s: SparseProfile) -> Self {
        pb::SparseProfile::from(&s)
    }
}

impl TryFrom<pb::SparseProfile> for SparseProfile {
    type Error = ConvertError;

    fn try_from(s: pb::SparseProfile) -> Result<Self> {
        Ok(SparseProfile {
            include: s
                .include
                .iter()
                .map(|p| path_from_proto("sparse.include", p))
                .collect::<Result<_>>()?,
            exclude: s
                .exclude
                .iter()
                .map(|p| path_from_proto("sparse.exclude", p))
                .collect::<Result<_>>()?,
        })
    }
}

impl From<&Workspace> for pb::Workspace {
    fn from(w: &Workspace) -> Self {
        pb::Workspace {
            id: id_to_proto(&w.id),
            repo: id_to_proto(&w.repo),
            owner: pb::Principal::from(&w.owner).into(),
            name: w.name.clone(),
            base: hash_to_proto(&w.base),
            working_change: id_to_proto(&w.working_change),
            snapshot: w.snapshot.as_ref().map(hash_to_proto),
            sparse: pb::SparseProfile::from(&w.sparse).into(),
            head_op: w.head_op.as_ref().map(id_to_proto),
            created_at: timestamp_to_proto(w.created_at),
            updated_at: timestamp_to_proto(w.updated_at),
        }
    }
}

impl From<Workspace> for pb::Workspace {
    fn from(w: Workspace) -> Self {
        pb::Workspace::from(&w)
    }
}

impl TryFrom<pb::Workspace> for Workspace {
    type Error = ConvertError;

    fn try_from(w: pb::Workspace) -> Result<Self> {
        Ok(Workspace {
            id: id_from_proto("workspace.id", &w.id)?,
            repo: id_from_proto("workspace.repo", &w.repo)?,
            owner: required("workspace.owner", w.owner)?.try_into()?,
            name: w.name,
            base: hash_from_proto("workspace.base", &w.base)?,
            working_change: id_from_proto("workspace.working_change", &w.working_change)?,
            snapshot: opt_hash_from_proto("workspace.snapshot", w.snapshot.as_ref())?,
            // A missing profile means "everything", like `SparseProfile::full`.
            sparse: w
                .sparse
                .map(SparseProfile::try_from)
                .transpose()?
                .unwrap_or_default(),
            head_op: opt_id_from_proto("workspace.head_op", w.head_op.as_ref())?,
            created_at: timestamp_from_proto(w.created_at),
            updated_at: timestamp_from_proto(w.updated_at),
        })
    }
}

impl From<&OperationKind> for pb::OperationKind {
    fn from(k: &OperationKind) -> Self {
        use pb::operation_kind as ok;
        let kind = match k {
            OperationKind::Create => ok::Kind::Create(ok::Create {}),
            OperationKind::Snapshot { tree } => ok::Kind::Snapshot(ok::Snapshot {
                tree: hash_to_proto(tree),
            }),
            OperationKind::NewChange { change, base } => ok::Kind::NewChange(ok::NewChange {
                change: id_to_proto(change),
                base: hash_to_proto(base),
            }),
            OperationKind::Describe { version } => ok::Kind::Describe(ok::Describe {
                version: hash_to_proto(version),
            }),
            OperationKind::Rebase { from, to } => ok::Kind::Rebase(ok::Rebase {
                from: hash_to_proto(from),
                to: hash_to_proto(to),
            }),
            OperationKind::Resolve { version } => ok::Kind::Resolve(ok::Resolve {
                version: hash_to_proto(version),
            }),
            OperationKind::SetSparse { profile } => ok::Kind::SetSparse(ok::SetSparse {
                profile: pb::SparseProfile::from(profile).into(),
            }),
            OperationKind::Handoff { to } => ok::Kind::Handoff(ok::Handoff {
                to: pb::Principal::from(to).into(),
            }),
            OperationKind::Undo { undone } => ok::Kind::Undo(ok::Undo {
                undone: id_to_proto(undone),
            }),
            OperationKind::Other { description } => ok::Kind::Other(ok::Other {
                description: description.clone(),
            }),
        };
        pb::OperationKind { kind: Some(kind) }
    }
}

impl TryFrom<pb::OperationKind> for OperationKind {
    type Error = ConvertError;

    fn try_from(k: pb::OperationKind) -> Result<Self> {
        use pb::operation_kind::Kind;
        Ok(match required("operation.kind", k.kind)? {
            Kind::Create(_) => OperationKind::Create,
            Kind::Snapshot(s) => OperationKind::Snapshot {
                tree: hash_from_proto("operation.kind.snapshot.tree", &s.tree)?,
            },
            Kind::NewChange(n) => OperationKind::NewChange {
                change: id_from_proto("operation.kind.new_change.change", &n.change)?,
                base: hash_from_proto("operation.kind.new_change.base", &n.base)?,
            },
            Kind::Describe(d) => OperationKind::Describe {
                version: hash_from_proto("operation.kind.describe.version", &d.version)?,
            },
            Kind::Rebase(r) => OperationKind::Rebase {
                from: hash_from_proto("operation.kind.rebase.from", &r.from)?,
                to: hash_from_proto("operation.kind.rebase.to", &r.to)?,
            },
            Kind::Resolve(r) => OperationKind::Resolve {
                version: hash_from_proto("operation.kind.resolve.version", &r.version)?,
            },
            Kind::SetSparse(s) => OperationKind::SetSparse {
                profile: s
                    .profile
                    .map(SparseProfile::try_from)
                    .transpose()?
                    .unwrap_or_default(),
            },
            Kind::Handoff(h) => OperationKind::Handoff {
                to: required("operation.kind.handoff.to", h.to)?.try_into()?,
            },
            Kind::Undo(u) => OperationKind::Undo {
                undone: id_from_proto("operation.kind.undo.undone", &u.undone)?,
            },
            Kind::Other(o) => OperationKind::Other {
                description: o.description,
            },
        })
    }
}

impl From<&Operation> for pb::Operation {
    fn from(op: &Operation) -> Self {
        pb::Operation {
            id: id_to_proto(&op.id),
            workspace: id_to_proto(&op.workspace),
            parent: op.parent.as_ref().map(id_to_proto),
            actor: pb::Principal::from(&op.actor).into(),
            kind: pb::OperationKind::from(&op.kind).into(),
            at: timestamp_to_proto(op.at),
            resulting: pb::Workspace::from(&op.resulting).into(),
        }
    }
}

impl From<Operation> for pb::Operation {
    fn from(op: Operation) -> Self {
        pb::Operation::from(&op)
    }
}

impl TryFrom<pb::Operation> for Operation {
    type Error = ConvertError;

    fn try_from(op: pb::Operation) -> Result<Self> {
        let actor: Principal = required("operation.actor", op.actor)?.try_into()?;
        Ok(Operation {
            id: id_from_proto("operation.id", &op.id)?,
            workspace: id_from_proto("operation.workspace", &op.workspace)?,
            parent: opt_id_from_proto("operation.parent", op.parent.as_ref())?,
            actor,
            kind: required("operation.kind", op.kind)?.try_into()?,
            resulting: required("operation.resulting", op.resulting)?.try_into()?,
            at: timestamp_from_proto(op.at),
        })
    }
}
