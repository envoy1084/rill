//! A planted credential is refused on every path that stores a version.

use rill_types::Principal;

use super::super::*;
use super::fixtures::*;

#[tokio::test(flavor = "multi_thread")]
async fn a_planted_secret_is_blocked_at_every_write() {
    let server = TestServer::start(admins_config()).await;
    let mut c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut c, "app").await;

    let inline = apply(
        &mut c,
        "app",
        vec![set_edit(".env", format!("AWS={AWS_KEY}\n").as_bytes())],
    )
    .await
    .unwrap_err();
    assert_eq!(inline.code, Code::FailedPrecondition);
    assert!(
        inline
            .message
            .as_deref()
            .unwrap_or_default()
            .contains("aws-access-token"),
        "{}",
        inline.message.as_deref().unwrap_or_default()
    );
    assert!(
        !inline
            .message
            .as_deref()
            .unwrap_or_default()
            .contains(AWS_KEY),
        "the refusal never repeats the secret"
    );

    // Deep inside a large uploaded file, past the first chunk boundary.
    let mut big = vec![b'x'; 3 << 20];
    big.extend_from_slice(format!("\nkey = {AWS_KEY}\n").as_bytes());
    let (edit, _) = blob(&server, "data/big.txt", &big).await;
    assert_eq!(
        apply(&mut c, "app", vec![edit]).await.unwrap_err().code,
        Code::FailedPrecondition
    );

    // A raw version naming the same content is refused too.
    let mut cv = rill_types::ChangeVersion {
        change_id: rill_types::ChangeId::new(),
        parents: Vec::new(),
        predecessor: None,
        subtree_changes: Default::default(),
        file_changes: Default::default(),
        author: Principal::human("root"),
        provenance: Default::default(),
        intent: None,
        message: "raw".into(),
        created_at: rill_types::Timestamp::now(),
        extra: Default::default(),
    };
    cv.file_changes.insert(
        rill_types::RepoPathBuf::new("k.txt").unwrap(),
        rill_types::FileChange::set(
            rill_types::ContentRef::inline(format!("{AWS_KEY}\n").into_bytes()),
            rill_types::FileKind::Regular,
        ),
    );
    let raw = c
        .changes
        .create_version(pb::CreateVersionRequest {
            parent: common::names::repo("app"),
            version: pb::ChangeVersion::from(cv).into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(raw.code, Code::FailedPrecondition);

    let ws = c
        .workspace
        .create_workspace(pb::CreateWorkspaceRequest {
            parent: common::names::repo("app"),
            workspace: (pb::Workspace {
                display_name: "w".into(),
                ..Default::default()
            })
            .into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .workspace
        .unwrap();
    let ctx = server.state.repos.resolve("app").await.unwrap();
    let mut builder = rill_core::TreeBuilder::new();
    builder.edit(
        &rill_types::RepoPathBuf::new("deploy/.env").unwrap(),
        rill_core::tree::Edit::Set(
            rill_types::ContentRef::inline(format!("AWS={AWS_KEY}\n").into_bytes()),
            rill_types::FileKind::Regular,
        ),
    );
    let tree = builder.build(&ctx.trees, None).await.unwrap();
    // A snapshot only records the working copy; the version is made when
    // it becomes a change, and that is refused.
    c.workspace
        .snapshot_workspace(pb::SnapshotWorkspaceRequest {
            request_id: String::new(),
            name: ws.name.clone(),
            tree_hash: Bytes::copy_from_slice(tree.as_bytes()),
        })
        .await
        .unwrap();
    let change = c
        .workspace
        .start_change(pb::StartChangeRequest {
            name: ws.name,
            message: "from the working copy".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(change.code, Code::FailedPrecondition, "{change:?}");

    // Clean content goes through, and the refusals are audited.
    apply(&mut c, "app", vec![set_edit("README", b"hello\n")])
        .await
        .unwrap();
    rill_runtime::audit::worker::catch_up(&server.state)
        .await
        .unwrap();
    let blocked = c
        .audit
        .list_audit_entries(pb::ListAuditEntriesRequest {
            parent: common::names::org("default"),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .entries
        .into_iter()
        .filter(|e| e.action == "secret.block")
        .count();
    assert_eq!(blocked, 4);
    server.stop().await;
}
