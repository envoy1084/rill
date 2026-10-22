//! Ways around the scanner found in review: content reached through an
//! allowed path, a subtree copy, the message, or a quarantined parent.

use rill_types::{ChangeVersion, ObjectHash, Principal, RepoPathBuf, SubtreeChange};

use super::super::*;
use super::fixtures::*;

const RULES: &str = "allow_paths = [\"tests/fixtures/**\"]\n";

/// A version of `repo` whose only change copies `from_path` of `source`
/// to each of `to`.
fn copies(source: &Bytes, from_path: &str, to: &[String]) -> pb::ChangeVersion {
    let source = ObjectHash::from_slice(source).unwrap();
    let mut cv = ChangeVersion {
        change_id: rill_types::ChangeId::new(),
        parents: vec![source],
        predecessor: None,
        subtree_changes: Default::default(),
        file_changes: Default::default(),
        author: Principal::human("root"),
        provenance: Default::default(),
        intent: None,
        message: "copy".into(),
        created_at: rill_types::Timestamp::now(),
        extra: Default::default(),
    };
    for dest in to {
        cv.subtree_changes.insert(
            RepoPathBuf::new(dest).unwrap(),
            SubtreeChange::Copy {
                from_path: RepoPathBuf::new(from_path).unwrap(),
                from_version: source,
            },
        );
    }
    pb::ChangeVersion::from(cv)
}

async fn create(c: &mut Clients, version: pb::ChangeVersion) -> Result<(), Status> {
    c.repo
        .create_change_version(pb::CreateChangeVersionRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            version: Some(version),
            signatures: Vec::new(),
        })
        .await
        .map(|_| ())
}

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

#[tokio::test(flavor = "multi_thread")]
async fn a_copy_from_an_unreadable_path_is_refused_before_it_is_scanned() {
    let server = TestServer::start(admins_config()).await;
    let mut root = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut root, "app").await;
    land_rules(&mut root, "app", "allow_paths = [\"infra/**\"]\n").await;
    let (infra, _) = blob(&server, "infra/deploy.env", &secret_file()).await;
    let source = apply(&mut root, "app", vec![infra]).await.unwrap().hash;
    let bob = Principal::human("bob");
    root.access
        .add_relation(pb::AddRelationRequest {
            operation_id: Bytes::new(),
            relation: Some(pb::Relation {
                repo: "app".into(),
                path: "scratch".into(),
                relation: "writer".into(),
                subject: format!("principal:{}", bob.id),
            }),
        })
        .await
        .unwrap();
    let mut bob = clients(&server, Some(&server.token_for(&bob))).await;
    let refused = create(&mut bob, copies(&source, "infra", &["scratch/x".into()]))
        .await
        .unwrap_err();
    assert_eq!(refused.code(), Code::PermissionDenied, "{refused:?}");
    assert!(!refused.message().contains("deploy.env"), "{refused:?}");
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn copied_files_count_toward_the_file_limit() {
    let server = TestServer::start(admins_config()).await;
    let mut c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut c, "app").await;
    let files: Vec<pb::Edit> = (0..1000)
        .map(|i| set_edit(&format!("lib/f{i}.txt"), format!("{i}\n").as_bytes()))
        .collect();
    let source = apply(&mut c, "app", files).await.unwrap().hash;
    let to: Vec<String> = (0..=rill_types::limits::MAX_FILES_PER_CHANGE / 1000)
        .map(|i| format!("copy{i}"))
        .collect();
    let started = std::time::Instant::now();
    let refused = create(&mut c, copies(&source, "lib", &to))
        .await
        .unwrap_err();
    assert_eq!(refused.code(), Code::InvalidArgument, "{refused:?}");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "refused before walking"
    );
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_credential_in_extension_fields_is_refused() {
    let server = TestServer::start(admins_config()).await;
    let mut c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut c, "app").await;
    let source = apply(&mut c, "app", vec![set_edit("README", b"hi\n")])
        .await
        .unwrap()
        .hash;
    let mut cv = copies(&source, "", &[]);
    cv.extra
        .insert("note".into(), format!("key {AWS_KEY}").into());
    let refused = create(&mut c, cv).await.unwrap_err();
    assert_eq!(refused.code(), Code::FailedPrecondition, "{refused:?}");
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn too_many_findings_to_review_are_refused_not_quarantined() {
    let server = TestServer::start(admins_config()).await;
    let mut c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut c, "app").await;
    land_rules(&mut c, "app", "mode = \"quarantine\"\n").await;
    let fakes: String = fake_aws_keys(30)
        .into_iter()
        .enumerate()
        .map(|(i, key)| format!("k{i} = {key}\n"))
        .collect();
    let (edit, _) = blob(&server, "aaa/fixtures.txt", fakes.as_bytes()).await;
    let refused = apply(&mut c, "app", vec![edit]).await.unwrap_err();
    assert_eq!(refused.code(), Code::FailedPrecondition, "{refused:?}");
    assert!(refused.message().contains("more"), "{refused:?}");
    server.stop().await;
}
