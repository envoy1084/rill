//! A mutation retried with its operation id runs once.

use super::*;

#[tokio::test]
async fn a_retried_mutation_returns_the_first_response() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::from_static(&[1; 16]),
            repo: "r".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    // The same CreateRepo again is a replay, not "already exists".
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::from_static(&[1; 16]),
            repo: "r".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();

    let edit = |op: u8, body: &'static [u8]| pb::ApplyEditsRequest {
        operation_id: Bytes::from(vec![op; 16]),
        repo: "r".into(),
        base: None,
        edits: vec![set_edit("a.txt", body)],
        message: "edit".into(),
        intent: None,
        provenance: None,
        change_id: None,
    };
    let first = c
        .repo
        .apply_edits(edit(2, b"one\n"))
        .await
        .unwrap()
        .into_inner();
    let again = c
        .repo
        .apply_edits(edit(2, b"one\n"))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(first, again);
    let reused = c.repo.apply_edits(edit(2, b"two\n")).await.unwrap_err();
    assert_eq!(reused.code(), Code::InvalidArgument);

    // A request that failed released its id: fixing it and retrying works.
    let bad = pb::ApplyEditsRequest {
        edits: vec![set_edit("CON/../x", b"x\n")],
        ..edit(3, b"x\n")
    };
    assert!(c.repo.apply_edits(bad).await.is_err());
    c.repo.apply_edits(edit(3, b"three\n")).await.unwrap();
    server.stop().await;
}
