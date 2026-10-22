//! Undo restores a workspace's whole state, and undoing an undo redoes.

use super::*;

#[tokio::test]
async fn undo_restores_every_field_and_undoing_it_redoes() {
    let server = TestServer::start(dev_config()).await;
    let c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "ws".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let ws = c
        .workspace
        .create_workspace(pb::CreateWorkspaceRequest {
            operation_id: Bytes::new(),
            repo: "ws".into(),
            name: "w".into(),
            base: None.into(),
            sparse: None.into(),
        })
        .await
        .unwrap()
        .into_owned()
        .workspace
        .unwrap();
    let narrowed = c
        .workspace
        .set_sparse_profile(pb::SetSparseProfileRequest {
            operation_id: Bytes::new(),
            repo: "ws".into(),
            workspace: ws.id.clone(),
            sparse: (pb::SparseProfile {
                include: vec!["src".into()],
                exclude: vec![],
            })
            .into(),
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(
        narrowed.workspace.unwrap().sparse.unwrap().include,
        vec!["src"]
    );

    let undo = |target: Option<Bytes>| pb::UndoRequest {
        operation_id: Bytes::new(),
        repo: "ws".into(),
        workspace: ws.id.clone(),
        target,
    };
    let undone = c.workspace.undo(undo(None)).await.unwrap().into_owned();
    let sparse = undone
        .workspace
        .unwrap()
        .sparse
        .into_option()
        .unwrap_or_default();
    assert!(
        sparse.include.is_empty(),
        "sparse profile restored: {sparse:?}"
    );

    let undo_op = undone.operation.unwrap().id;
    let redone = c
        .workspace
        .undo(undo(Some(undo_op)))
        .await
        .unwrap()
        .into_owned();
    assert_eq!(
        redone.workspace.unwrap().sparse.unwrap().include,
        vec!["src"]
    );
    server.stop().await;
}
