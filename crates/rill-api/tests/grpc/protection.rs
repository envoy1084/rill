//! Secret scanning at every write, quarantine, and redaction.

use rill_types::{ObjectHash, ObjectKind, Principal};

use super::*;

const AWS_KEY: &str = "AKIAZ7T3KQ2MVXC4L9PW";

fn admins_config() -> ServerConfig {
    let mut config = ServerConfig::default();
    config.identity.admins = vec!["root".into(), "sec".into()];
    config
}

async fn repo(c: &mut Clients, name: &str) {
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: name.into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
}

async fn apply(
    c: &mut Clients,
    repo: &str,
    edits: Vec<pb::Edit>,
) -> Result<pb::ApplyEditsResponse, Status> {
    c.repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: None,
            edits,
            message: "edit".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .map(|r| r.into_inner())
}

/// An edit storing `bytes` as a blob uploaded beforehand.
async fn blob(server: &TestServer, path: &str, bytes: &[u8]) -> (pb::Edit, ObjectHash) {
    let hash = ObjectHash::digest(ObjectKind::Blob, bytes);
    server
        .state
        .stores
        .objects
        .put(ObjectKind::Blob, &hash, Bytes::copy_from_slice(bytes))
        .await
        .unwrap();
    let edit = pb::Edit {
        path: path.into(),
        op: Some(pb::edit::Op::SetContent(pb::SetContent {
            content: Some(pb::ContentRef {
                r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                    hash: Bytes::copy_from_slice(hash.as_bytes()),
                    size: bytes.len() as u64,
                })),
            }),
            kind: pb::FileKind::Regular as i32,
            copy_from: None,
        })),
    };
    (edit, hash)
}

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
    assert_eq!(inline.code(), Code::FailedPrecondition);
    assert!(
        inline.message().contains("aws-access-key-id"),
        "{}",
        inline.message()
    );
    assert!(
        !inline.message().contains(AWS_KEY),
        "the refusal never repeats the secret"
    );

    // Deep inside a large uploaded file, past the first chunk boundary.
    let mut big = vec![b'x'; 3 << 20];
    big.extend_from_slice(format!("\nkey = {AWS_KEY}\n").as_bytes());
    let (edit, _) = blob(&server, "data/big.txt", &big).await;
    assert_eq!(
        apply(&mut c, "app", vec![edit]).await.unwrap_err().code(),
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
        .repo
        .create_change_version(pb::CreateChangeVersionRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            version: Some(pb::ChangeVersion::from(cv)),
            signatures: Vec::new(),
        })
        .await
        .unwrap_err();
    assert_eq!(raw.code(), Code::FailedPrecondition);

    let ws = c
        .workspace
        .create_workspace(pb::CreateWorkspaceRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            name: "w".into(),
            base: None,
            sparse: None,
        })
        .await
        .unwrap()
        .into_inner()
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
        .snapshot(pb::SnapshotRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            workspace: ws.id.clone(),
            tree_hash: Bytes::copy_from_slice(tree.as_bytes()),
        })
        .await
        .unwrap();
    let change = c
        .workspace
        .new_change(pb::NewChangeRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            workspace: ws.id,
            message: "from the working copy".into(),
            intent: None,
            provenance: None,
        })
        .await
        .unwrap_err();
    assert_eq!(change.code(), Code::FailedPrecondition, "{change:?}");

    // Clean content goes through, and the refusals are audited.
    apply(&mut c, "app", vec![set_edit("README", b"hello\n")])
        .await
        .unwrap();
    rill_api::audit::worker::catch_up(&server.state)
        .await
        .unwrap();
    let blocked = c
        .audit
        .list_audit_entries(pb::ListAuditEntriesRequest {
            org: "default".into(),
            after_seq: 0,
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner()
        .entries
        .into_iter()
        .filter(|e| e.action == "secret.block")
        .count();
    assert_eq!(blocked, 4);
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn quarantined_versions_wait_for_an_owner() {
    let server = TestServer::start(admins_config()).await;
    let mut c = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    repo(&mut c, "app").await;
    let rules = apply(
        &mut c,
        "app",
        vec![set_edit(".rill/secrets.toml", b"mode = \"quarantine\"\n")],
    )
    .await
    .unwrap();
    let rules_change = rules.version.unwrap().change_id;
    land_with(&mut c, "app", rules_change, &[("Owner", 1)]).await;

    let held = apply(
        &mut c,
        "app",
        vec![set_edit(
            "deploy.env",
            format!("AWS={AWS_KEY}\n").as_bytes(),
        )],
    )
    .await
    .expect("quarantine stores the version");
    let version = held.hash.clone();
    let change = held.version.unwrap().change_id;
    c.review
        .submit_for_review(pb::SubmitForReviewRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            change_id: change.clone(),
        })
        .await
        .unwrap();
    for (label, value) in [("Verified", 1), ("Review", 2)] {
        c.review
            .vote(pb::VoteRequest {
                operation_id: Bytes::new(),
                repo: "app".into(),
                change_id: change.clone(),
                label: label.into(),
                value,
                reason: None,
                version: None,
            })
            .await
            .unwrap();
    }
    let ticket = c
        .review
        .request_land(pb::RequestLandRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            change_id: change.clone(),
        })
        .await
        .unwrap()
        .into_inner()
        .ticket
        .unwrap();
    let mut refused = None;
    for _ in 0..100 {
        let status = c
            .review
            .get_land_status(pb::GetLandStatusRequest {
                repo: "app".into(),
                ticket: ticket.id.clone(),
            })
            .await
            .unwrap()
            .into_inner();
        match pb::LandStatus::try_from(status.ticket.as_ref().unwrap().status).unwrap() {
            pb::LandStatus::Landed => panic!("a quarantined version landed"),
            pb::LandStatus::Failed | pb::LandStatus::Cancelled => {
                refused = status.error;
                break;
            }
            _ => tokio::time::sleep(std::time::Duration::from_millis(50)).await,
        }
    }
    assert!(refused.expect("refused").contains("quarantined"));

    // Nor can an importer append it to trunk.
    let head = server
        .state
        .repos
        .resolve("app")
        .await
        .unwrap()
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
            version: version.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(appended.code(), Code::FailedPrecondition, "{appended:?}");

    c.protection
        .clear_quarantine(pb::ClearQuarantineRequest {
            operation_id: Bytes::new(),
            repo: "app".into(),
            version,
        })
        .await
        .unwrap();
    assert!(server
        .state
        .repos
        .resolve("app")
        .await
        .unwrap()
        .repo
        .quarantined(&ObjectHash::from_slice(&held.hash).unwrap())
        .await
        .unwrap()
        .is_none());
    server.stop().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_redacted_object_needs_two_admins_and_is_never_served() {
    let server = TestServer::start(admins_config()).await;
    let mut root = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    let mut sec = clients(&server, Some(&server.token_for(&Principal::human("sec")))).await;
    repo(&mut root, "app").await;
    let leaked = "customer list: alice, bob\n".repeat(40);
    let (edit, hash) = blob(&server, "data/customers.txt", leaked.as_bytes()).await;
    let version = apply(&mut root, "app", vec![edit]).await.unwrap().hash;
    let at = pb::RootSpec {
        root: Some(pb::root_spec::Root::VersionHash(version.clone())),
    };
    let grant = root
        .repo
        .resolve_path(pb::ResolvePathRequest {
            repo: "app".into(),
            root: Some(at.clone()),
            path: "data/customers.txt".into(),
        })
        .await
        .unwrap()
        .into_inner()
        .read_grant;
    let read = |c: &mut Clients| {
        let mut repo = c.repo.clone();
        let grant = grant.clone();
        let leaked = leaked.clone();
        async move {
            repo.get_blob(pb::GetBlobRequest {
                repo: "app".into(),
                content: Some(pb::ContentRef {
                    r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                        hash: Bytes::copy_from_slice(hash.as_bytes()),
                        size: leaked.len() as u64,
                    })),
                }),
                offset: None,
                length: None,
                grant,
            })
            .await?
            .into_inner()
            .message()
            .await
        }
    };
    assert!(read(&mut root).await.unwrap().is_some());

    let request = root
        .protection
        .request_redaction(pb::RequestRedactionRequest {
            operation_id: Bytes::new(),
            org: "default".into(),
            kind: pb::ObjectKind::Blob as i32,
            hash: Bytes::copy_from_slice(hash.as_bytes()),
            reason: "customer data committed by mistake".into(),
        })
        .await
        .unwrap()
        .into_inner()
        .redaction
        .unwrap();
    let approve = |c: &mut Clients| {
        let mut p = c.protection.clone();
        let id = request.id.clone();
        async move {
            p.approve_redaction(pb::ApproveRedactionRequest {
                operation_id: Bytes::new(),
                org: "default".into(),
                id,
            })
            .await
        }
    };
    assert_eq!(
        approve(&mut root).await.unwrap_err().code(),
        Code::PermissionDenied,
        "not one's own"
    );
    assert!(
        read(&mut root).await.unwrap().is_some(),
        "pending is not applied"
    );
    approve(&mut sec).await.unwrap();

    assert_eq!(
        read(&mut root).await.unwrap_err().code(),
        Code::FailedPrecondition
    );
    let token = server.token_for(&Principal::human("root"));
    let http = reqwest::Client::new()
        .get(format!(
            "http://{}/o/app/blob/{}",
            server.http,
            hash.to_hex()
        ))
        .bearer_auth(&token)
        .header("x-rill-grant", &grant)
        .send()
        .await
        .unwrap();
    assert_eq!(http.status().as_u16(), 451);
    // Its content cannot come back under the same hash.
    let again = apply(
        &mut root,
        "app",
        vec![set_edit("copy.txt", leaked.as_bytes())],
    )
    .await;
    assert!(again.is_err(), "re-adding redacted content is refused");
    // Search skips it rather than failing.
    let hits = root
        .repo
        .grep(pb::GrepRequest {
            repo: "app".into(),
            root: Some(at),
            pattern: "customer".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_inner()
        .hits;
    assert!(hits.is_empty(), "{hits:?}");
    let published: serde_json::Value = reqwest::get(format!("http://{}/redactions", server.http))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(published["hashes"][0], hash.to_hex());

    rill_api::audit::worker::catch_up(&server.state)
        .await
        .unwrap();
    let actions: Vec<String> = root
        .audit
        .list_audit_entries(pb::ListAuditEntriesRequest {
            org: "default".into(),
            after_seq: 0,
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner()
        .entries
        .into_iter()
        .map(|e| e.action)
        .collect();
    for action in ["redaction.request", "redaction.apply"] {
        assert!(
            actions.iter().any(|a| a == action),
            "{action} in {actions:?}"
        );
    }
    server.stop().await;
}
