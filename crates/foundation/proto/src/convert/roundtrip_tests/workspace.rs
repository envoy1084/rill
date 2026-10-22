//! Workspaces and every operation kind.

use rill_types::*;

use super::fixtures::{h, p, wire};
use crate::error::ConvertError;
use crate::rill::v1 as pb;

fn workspace_with(snapshot: Option<ObjectHash>) -> Workspace {
    Workspace {
        id: WorkspaceId::derived("test", "ws"),
        repo: RepoId::derived("test", "repo"),
        owner: Principal::agent("coder"),
        name: "feature-x".into(),
        base: h("base"),
        working_change: ChangeId::derived("test", "wc"),
        sparse: SparseProfile {
            include: vec![p("src")],
            exclude: vec![p("src/vendor")],
        },
        head_op: Some(OperationId::derived("test", "op")),
        created_at: Timestamp::from_millis(1),
        updated_at: Timestamp::from_millis(2),
        snapshot,
    }
}

#[test]
fn workspace_roundtrip() {
    let ws = workspace_with(Some(ObjectHash::digest(ObjectKind::Tree, b"t")));
    assert_eq!(
        Workspace::try_from(wire(pb::Workspace::from(&ws))).unwrap(),
        ws
    );

    let mut full = ws.clone();
    full.sparse = SparseProfile::full();
    full.snapshot = None;
    full.head_op = None;
    let mut w = wire(pb::Workspace::from(&full));
    w.sparse = None;
    assert_eq!(Workspace::try_from(w).unwrap(), full);
}

#[test]
fn operation_roundtrip_every_kind() {
    let kinds = vec![
        OperationKind::Create,
        OperationKind::Snapshot { tree: h("t") },
        OperationKind::NewChange {
            change: ChangeId::derived("test", "c"),
            base: h("b"),
        },
        OperationKind::Describe { version: h("v") },
        OperationKind::Rebase {
            from: h("a"),
            to: h("b"),
        },
        OperationKind::Resolve { version: h("v") },
        OperationKind::SetSparse {
            profile: SparseProfile {
                include: vec![p("a")],
                exclude: vec![],
            },
        },
        OperationKind::Handoff {
            to: Principal::human("bob"),
        },
        OperationKind::Undo {
            undone: OperationId::derived("test", "u"),
        },
        OperationKind::Other {
            description: "import".into(),
        },
    ];
    for (i, kind) in kinds.into_iter().enumerate() {
        let op = Operation {
            id: OperationId::derived("test", &i.to_string()),
            workspace: WorkspaceId::derived("test", "ws"),
            parent: (i > 0).then(|| OperationId::derived("test", &(i - 1).to_string())),
            actor: Principal::agent("coder"),
            kind,
            resulting: workspace_with((i % 2 == 0).then(|| h("snap"))),
            at: Timestamp::from_millis(i as i64),
        };
        assert_eq!(
            Operation::try_from(wire(pb::Operation::from(&op))).unwrap(),
            op
        );
    }

    let bad = pb::Operation {
        kind: None,
        ..pb::Operation::from(Operation {
            id: OperationId::derived("t", "x"),
            workspace: WorkspaceId::derived("t", "w"),
            parent: None,
            actor: Principal::human("a"),
            kind: OperationKind::Create,
            resulting: workspace_with(None),
            at: Timestamp::from_millis(0),
        })
    };
    assert_eq!(
        Operation::try_from(bad).unwrap_err(),
        ConvertError::missing("operation.kind")
    );
}
