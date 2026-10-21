//! Ways around the scanner found in review: content reached through an
//! allowed path, a subtree copy, the message, or a quarantined parent.

use rill_types::{ChangeVersion, ObjectHash, Principal, RepoPathBuf, SubtreeChange};

use super::super::*;
use super::fixtures::*;

const RULES: &str = "allow_paths = [\"tests/fixtures/**\"]\n";

fn secret_file() -> Vec<u8> {
    let mut bytes = "# fixture\n".repeat(40).into_bytes();
    bytes.extend_from_slice(format!("aws = {AWS_KEY}\n").as_bytes());
    bytes
}

#[tokio::test(flavor = "multi_thread")]
async fn content_allowed_at_one_path_is_scanned_at_another() {
    let server = TestServer::start(admins_config()).await;
    let mut c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut c, "app").await;
    land_rules(&mut c, "app", RULES).await;
    let (fixture, _) = blob(&server, "tests/fixtures/k.txt", &secret_file()).await;
    apply(&mut c, "app", vec![fixture])
        .await
        .expect("an allowed path");
    let (moved, _) = blob(&server, "src/k.txt", &secret_file()).await;
    let refused = apply(&mut c, "app", vec![moved]).await.unwrap_err();
    assert_eq!(refused.code(), Code::FailedPrecondition, "{refused:?}");
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_subtree_copy_is_scanned_at_its_destination() {
    let server = TestServer::start(admins_config()).await;
    let mut c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut c, "app").await;
    land_rules(&mut c, "app", RULES).await;
    let (fixture, _) = blob(&server, "tests/fixtures/k.txt", &secret_file()).await;
    let source = apply(&mut c, "app", vec![fixture]).await.unwrap().hash;

    let mut cv = ChangeVersion {
        change_id: rill_types::ChangeId::new(),
        parents: vec![ObjectHash::from_slice(&source).unwrap()],
        predecessor: None,
        subtree_changes: Default::default(),
        file_changes: Default::default(),
        author: Principal::human("root"),
        provenance: Default::default(),
        intent: None,
        message: "copy fixtures".into(),
        created_at: rill_types::Timestamp::now(),
        extra: Default::default(),
    };
    cv.subtree_changes.insert(
        RepoPathBuf::new("src/keys").unwrap(),
        SubtreeChange::Copy {
            from_path: RepoPathBuf::new("tests/fixtures").unwrap(),
            from_version: ObjectHash::from_slice(&source).unwrap(),
        },
    );
    let refused = c
        .repo
        .create_change_version(pb::CreateChangeVersionRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            version: Some(pb::ChangeVersion::from(cv)),
            signatures: Vec::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(refused.code(), Code::FailedPrecondition, "{refused:?}");
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_credential_in_the_message_is_refused() {
    let server = TestServer::start(admins_config()).await;
    let mut c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut c, "app").await;
    let refused = apply_on(
        &mut c,
        "app",
        None,
        vec![set_edit("README", b"hi\n")],
        &format!("rotate the key {AWS_KEY}"),
    )
    .await
    .unwrap_err();
    assert_eq!(refused.code(), Code::FailedPrecondition, "{refused:?}");
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_child_of_a_quarantined_version_cannot_reach_trunk() {
    let server = TestServer::start(admins_config()).await;
    let mut c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut c, "app").await;
    land_rules(&mut c, "app", "mode = \"quarantine\"\n").await;
    let held = apply(
        &mut c,
        "app",
        vec![set_edit(".env", format!("{AWS_KEY}\n").as_bytes())],
    )
    .await
    .unwrap()
    .hash;
    let child = apply_on(
        &mut c,
        "app",
        Some(&held),
        vec![set_edit("README", b"hi\n")],
        "clean",
    )
    .await
    .unwrap()
    .hash;
    let ctx = server.state.repos.resolve("app").await.unwrap();
    let head = ctx
        .repo
        .trunk_head()
        .await
        .unwrap()
        .map(|(_, h)| Bytes::copy_from_slice(h.as_bytes()));
    let appended = c
        .admin
        .append_trunk(pb::AppendTrunkRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            expected: head,
            version: child,
        })
        .await
        .unwrap_err();
    assert_eq!(appended.code(), Code::FailedPrecondition, "{appended:?}");
    server.stop().await;
}
