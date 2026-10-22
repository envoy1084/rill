//! A mutation retried with its operation id runs once.

use super::*;

#[tokio::test]
async fn a_retried_mutation_returns_the_first_response() {
    let server = TestServer::start(dev_config()).await;
    let c = clients(&server, None).await;
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
        base: None.into(),
        edits: vec![set_edit("a.txt", body)],
        message: "edit".into(),
        intent: None,
        provenance: None.into(),
        change_id: None,
    };
    let first = c
        .repo
        .apply_edits(edit(2, b"one\n"))
        .await
        .unwrap()
        .into_owned();
    let again = c
        .repo
        .apply_edits(edit(2, b"one\n"))
        .await
        .unwrap()
        .into_owned();
    assert_eq!(first, again);
    let reused = c.repo.apply_edits(edit(2, b"two\n")).await.unwrap_err();
    assert_eq!(reused.code, Code::InvalidArgument);

    // A request that failed released its id: fixing it and retrying works.
    let bad = pb::ApplyEditsRequest {
        edits: vec![set_edit("CON/../x", b"x\n")],
        ..edit(3, b"x\n")
    };
    assert!(c.repo.apply_edits(bad).await.is_err());
    c.repo.apply_edits(edit(3, b"three\n")).await.unwrap();
    server.stop().await;
}

#[tokio::test]
async fn a_retry_after_a_crash_returns_the_version_already_committed() {
    let server = TestServer::start(dev_config()).await;
    let c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "crash".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let edit = pb::ApplyEditsRequest {
        operation_id: Bytes::from_static(&[9; 16]),
        repo: "crash".into(),
        base: None.into(),
        edits: vec![set_edit("a.txt", b"once\n")],
        message: "edit".into(),
        intent: None,
        provenance: None.into(),
        change_id: None,
    };
    let first = c.repo.apply_edits(edit.clone()).await.unwrap().into_owned();

    // The server committed the version, then died before replying.
    let dev = rill_types::Principal::human("dev").id.to_string();
    server
        .state
        .stores
        .idem_rewind(&dev, &[9; 16], &first.hash)
        .await
        .unwrap();
    let retried = c.repo.apply_edits(edit).await.unwrap().into_owned();
    assert_eq!(retried.hash, first.hash);

    let stores = &server.state.stores;
    let repo = stores.repo(stores.get_repo_by_name("crash").await.unwrap().unwrap().id);
    assert_eq!(
        repo.log_after(None, 10).await.unwrap().len(),
        1,
        "one version, not two"
    );
    server.stop().await;
}
