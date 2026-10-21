//! Input limits at the gRPC edge.

use super::*;

#[tokio::test]
async fn large_inline_edits_become_blobs_and_huge_messages_are_refused() {
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: "lim".into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let version = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "lim".into(),
            base: None,
            edits: vec![set_edit("big.txt", &[b'x'; 4096])],
            message: "big".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner()
        .version
        .unwrap();
    let (_, change) = version.file_changes.iter().next().unwrap();
    let set = match &change.change {
        Some(pb::file_change::Change::Set(set)) => set,
        other => panic!("{other:?}"),
    };
    assert!(
        matches!(
            set.content.as_ref().and_then(|c| c.r#ref.as_ref()),
            Some(pb::content_ref::Ref::Blob(_))
        ),
        "4 KiB sent inline is stored as a blob"
    );

    let huge = vec![b'y'; 20 << 20];
    let err = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: "lim".into(),
            base: None,
            edits: vec![set_edit("huge.txt", &huge)],
            message: "huge".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.code(), Code::OutOfRange, "{err}");
    server.stop().await;
}
