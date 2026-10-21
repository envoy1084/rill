//! Redaction takes two administrators and stops every tier serving an
//! object.

use rill_types::Principal;

use super::super::*;
use super::fixtures::*;

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
    // The batch route answers "missing" for it: kind, hash, then u64::MAX.
    let batch = reqwest::Client::new()
        .post(format!("http://{}/o/app/batch", server.http))
        .bearer_auth(&token)
        .header("x-rill-grant", &grant)
        .json(&serde_json::json!([{ "kind": "blob", "hash": hash.to_hex() }]))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(batch.len(), 1 + 32 + 8, "no content in the frame");
    assert_eq!(&batch[33..], &u64::MAX.to_be_bytes());
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

#[tokio::test(flavor = "multi_thread")]
async fn a_redacted_tree_is_refused_even_when_cached() {
    let server = TestServer::start(admins_config()).await;
    let mut root = clients(&server, Some(&server.token_for(&Principal::human("root")))).await;
    let mut sec = clients(&server, Some(&server.token_for(&Principal::human("sec")))).await;
    repo(&mut root, "app").await;
    let version = apply(
        &mut root,
        "app",
        vec![set_edit("private/names.txt", b"alice\nbob\n")],
    )
    .await
    .unwrap()
    .hash;
    let at = pb::RootSpec {
        root: Some(pb::root_spec::Root::VersionHash(version.clone())),
    };
    let list = |c: &mut Clients| {
        let mut repo = c.repo.clone();
        let at = at.clone();
        async move {
            repo.list_dir(pb::ListDirRequest {
                repo: "app".into(),
                root: Some(at),
                path: "private".into(),
            })
            .await
        }
    };
    list(&mut root).await.expect("read once, so it is cached");

    let ctx = server.state.repos.resolve("app").await.unwrap();
    let version = rill_types::ObjectHash::from_slice(&version).unwrap();
    let tree = ctx.derive.get::<rill_core::RootTree>(&version).await.unwrap();
    let dir = ctx
        .trees
        .dir_hash(tree, &rill_types::RepoPathBuf::new("private").unwrap())
        .await
        .unwrap()
        .unwrap();
    let id = root
        .protection
        .request_redaction(pb::RequestRedactionRequest {
            operation_id: Bytes::new(),
            org: "default".into(),
            kind: pb::ObjectKind::Tree as i32,
            hash: Bytes::copy_from_slice(dir.as_bytes()),
            reason: "names".into(),
        })
        .await
        .unwrap()
        .into_inner()
        .redaction
        .unwrap()
        .id;
    sec.protection
        .approve_redaction(pb::ApproveRedactionRequest {
            operation_id: Bytes::new(),
            org: "default".into(),
            id,
        })
        .await
        .unwrap();
    let refused = list(&mut root).await.unwrap_err();
    assert_eq!(refused.code(), Code::FailedPrecondition, "{refused:?}");
    server.stop().await;
}
