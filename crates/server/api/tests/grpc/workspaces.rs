//! Undo restores a workspace's whole state, and undoing an undo redoes.

use super::*;

#[tokio::test]
async fn undo_restores_every_field_and_undoing_it_redoes() {
    let server = TestServer::start(dev_config()).await;
    let c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            request_id: String::new(),
            parent: common::names::org("default"),
            repo_id: "ws".into(),
            repo: None.into(),
        })
        .await
        .unwrap();
    let ws = c
        .workspace
        .create_workspace(pb::CreateWorkspaceRequest {
            request_id: String::new(),
            parent: common::names::repo("ws"),
            workspace: (pb::Workspace {
                display_name: "w".into(),
                ..Default::default()
            })
            .into(),
            base: String::new(),
        })
        .await
        .unwrap()
        .into_owned()
        .workspace
        .unwrap();
    let narrowed = c
        .workspace
        .update_workspace(pb::UpdateWorkspaceRequest {
            request_id: String::new(),
            workspace: (pb::Workspace {
                name: ws.name.clone(),
                sparse: (pb::SparseProfile {
                    include: vec!["src".into()],
                    exclude: vec![],
                })
                .into(),
                ..Default::default()
            })
            .into(),
            etag: ws.etag.clone(),
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(
        narrowed.workspace.unwrap().sparse.unwrap().include,
        vec!["src"]
    );

    let undo = |operation: String| pb::UndoOperationRequest {
        request_id: String::new(),
        name: operation,
    };
    let undone = c
        .workspace
        .undo_operation(undo(format!("{}/operations/head", ws.name)))
        .await
        .unwrap()
        .into_owned();
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

    let undo_op = undone.operation.unwrap().name;
    let redone = c
        .workspace
        .undo_operation(undo(undo_op))
        .await
        .unwrap()
        .into_owned();
    assert_eq!(
        redone.workspace.unwrap().sparse.unwrap().include,
        vec!["src"]
    );
    server.stop().await;
}
