//! Input limits at the gRPC edge.

use super::*;

#[tokio::test]
async fn large_inline_edits_become_blobs_and_huge_messages_are_refused() {
    let server = TestServer::start(dev_config()).await;
    let c = clients(&server, None).await;
    c.repo
        .create_repo(pb::CreateRepoRequest {
            parent: common::names::org("default"),
            repo_id: "lim".into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let version = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("lim"),
            base: None.into(),
            edits: vec![set_edit("big.txt", &[b'x'; 4096])],
            message: "big".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .version
        .unwrap();
    let (_, change) = version.file_changes.iter().next().unwrap();
    let set = match &change.change {
        Some(pb::file_change::Change::Set(set)) => set,
        other => panic!("{other:?}"),
    };
    assert!(
        matches!(
            set.content.as_option().and_then(|c| c.r#ref.as_ref()),
            Some(pb::content_ref::Ref::Blob(_))
        ),
        "4 KiB sent inline is stored as a blob"
    );

    let huge = vec![b'y'; 20 << 20];
    let err = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo("lim"),
            base: None.into(),
            edits: vec![set_edit("huge.txt", &huge)],
            message: "huge".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(err.code, Code::ResourceExhausted, "{err}");
    server.stop().await;
}

#[tokio::test]
async fn a_burst_past_the_rate_limit_is_told_when_to_retry() {
    let mut config = dev_config();
    config.rate_limit.per_principal_per_sec = 1;
    config.rate_limit.burst = 3;
    let server = TestServer::start(config).await;
    let c = clients(&server, None).await;
    let mut refused = Vec::new();
    for _ in 0..6 {
        if let Err(status) = c.identity.get_caller(pb::GetCallerRequest {}).await {
            refused.push(status);
        }
    }
    assert!(
        (2..=3).contains(&refused.len()),
        "a burst of three, then about one a second: {refused:?}"
    );
    for status in &refused {
        assert_eq!(status.code, Code::ResourceExhausted);
        assert!(status.response_headers().get("retry-after").is_some());
    }
    server.stop().await;
}
