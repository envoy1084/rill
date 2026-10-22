//! Authentication on both listeners.

use super::*;

#[tokio::test]
async fn issued_tokens_are_bound_by_relations() {
    let server = TestServer::start(ServerConfig::default()).await;
    let alice_token = server.token_for(&rill_types::Principal::human("alice"));
    let mallory_token = server.token_for(&rill_types::Principal::agent("mallory"));

    // No token at all: unauthenticated.
    let anon = clients(&server, None).await;
    let err = anon
        .repo
        .list_repos(pb::ListReposRequest {
            limit: 0,
            page_token: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, Code::Unauthenticated);

    let alice = clients(&server, Some(&alice_token)).await;
    alice
        .repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "private".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let relations = alice
        .access
        .list_relations(pb::ListRelationsRequest {
            repo: "private".into(),
            path: None,
            subject: None,
            relation: None,
        })
        .await
        .unwrap()
        .into_owned()
        .relations;
    assert_eq!(relations.len(), 1);
    assert_eq!(relations[0].relation, "owner");

    let mallory = clients(&server, Some(&mallory_token)).await;
    let err = mallory
        .repo
        .list_dir(pb::ListDirRequest {
            repo: "private".into(),
            root: None.into(),
            path: String::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, Code::PermissionDenied);
    let visible = mallory
        .repo
        .list_repos(pb::ListReposRequest {
            limit: 0,
            page_token: None,
        })
        .await
        .unwrap()
        .into_owned();
    assert!(visible.repos.is_empty());

    // Granting a reader relation opens reads but not writes.
    alice
        .access
        .add_relation(pb::AddRelationRequest {
            operation_id: Bytes::new(),
            relation: (pb::Relation {
                repo: "private".into(),
                path: String::new(),
                relation: "reader".into(),
                subject: format!("principal:{}", rill_types::Principal::agent("mallory").id),
            })
            .into(),
        })
        .await
        .unwrap();
    mallory
        .repo
        .list_dir(pb::ListDirRequest {
            repo: "private".into(),
            root: None.into(),
            path: String::new(),
        })
        .await
        .unwrap();
    let err = mallory
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "private".into(),
            base: None.into(),
            edits: vec![set_edit("x", b"y")],
            message: "nope".into(),
            intent: None,
            provenance: None.into(),
            change_id: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, Code::PermissionDenied);

    // Object protocol honours the same rules.
    let mut stream = TcpStream::connect(server.http).await.unwrap();
    stream
        .write_all(b"GET /o/private/blob/00 HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    assert!(raw.starts_with("HTTP/1.1 401"), "{raw}");

    server.stop().await;
}
