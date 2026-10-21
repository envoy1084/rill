//! One pass through every service, on each metadata backend.

use super::*;

#[tokio::test]
async fn dev_mode_end_to_end() {
    end_to_end(dev_config()).await;
}

#[cfg(feature = "fdb")]
#[tokio::test]
async fn end_to_end_on_foundationdb() {
    let root = format!("rill-test-{}", hex::encode(rill_types::ids::random_id()));
    let config = ServerConfig {
        metadata: rill_api::config::MetadataConfig::Fdb {
            cluster_file: std::env::var("RILL_FDB_CLUSTER_FILE").ok().map(Into::into),
            root: root.clone(),
        },
        ..dev_config()
    };
    end_to_end(config).await;
    let cluster = std::env::var("RILL_FDB_CLUSTER_FILE").ok();
    rill_store::FdbMetaStore::connect(cluster.as_deref(), &root)
        .unwrap()
        .engine()
        .clear_all()
        .await
        .unwrap();
}

async fn end_to_end(config: ServerConfig) {
    let server = TestServer::start(config).await;
    let mut c = clients(&server, None).await;
    let repo = "main";

    let created = c
        .repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            description: Some("smoke".into()),
            windows_safe: false,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(created.repo.unwrap().name, "main");

    // Watch trunk before anything lands so the event is observed live.
    let mut trunk_watch = c
        .repo
        .watch_trunk(pb::WatchTrunkRequest {
            repo: repo.into(),
            after_seq: Some(0),
        })
        .await
        .unwrap()
        .into_inner();

    let first = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: None,
            edits: vec![
                set_edit("README.md", b"hello\nworld\n"),
                set_edit("src/lib.rs", b"fn a() {}\n"),
            ],
            message: "initial".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner();
    let change_id = first.version.as_ref().unwrap().change_id.clone();
    assert!(first.version.unwrap().parents.is_empty());

    let seq = land(&mut c, repo, change_id.clone()).await;
    assert_eq!(seq, 1);

    let landed = tokio::time::timeout(Duration::from_secs(5), trunk_watch.message())
        .await
        .expect("trunk event")
        .unwrap()
        .unwrap();
    assert_eq!(landed.seq, 1);
    assert_eq!(landed.change_id, change_id);

    let listing = c
        .repo
        .list_dir(pb::ListDirRequest {
            repo: repo.into(),
            root: Some(trunk()),
            path: String::new(),
        })
        .await
        .unwrap()
        .into_inner();
    let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["README.md", "src"]);

    // A second change on top of trunk, amended through a patch.
    let second = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: Some(pb::EditBase {
                base: Some(pb::edit_base::Base::Root(trunk())),
            }),
            edits: vec![pb::Edit {
                path: "README.md".into(),
                op: Some(pb::edit::Op::Patch(
                    "--- a/README.md\n+++ b/README.md\n@@ -1,2 +1,2 @@\n hello\n-world\n+there\n"
                        .into(),
                )),
            }],
            message: "patch readme".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner();

    let diff = c
        .repo
        .diff(pb::DiffRequest {
            repo: repo.into(),
            base: Some(trunk()),
            target: Some(pb::RootSpec {
                root: Some(pb::root_spec::Root::VersionHash(second.hash.clone())),
            }),
            paths: vec![],
            format: pb::DiffFormat::Structured as i32,
            context_lines: 0,
            limit: 0,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "README.md");
    let hunk = &diff.files[0].hunks[0];
    let ops: Vec<i32> = hunk.lines.iter().map(|l| l.op).collect();
    assert!(ops.contains(&(pb::diff_line::Op::Delete as i32)));
    assert!(ops.contains(&(pb::diff_line::Op::Insert as i32)));

    let blame = c
        .repo
        .blame(pb::BlameRequest {
            repo: repo.into(),
            hash: second.hash.clone(),
            path: "README.md".into(),
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(blame.lines.len(), 2);
    assert_eq!(blame.lines[0].version, first.hash);
    assert_eq!(blame.lines[0].trunk_seq, Some(1));
    assert_eq!(blame.lines[1].version, second.hash);
    assert_eq!(blame.lines[1].text, Bytes::from_static(b"there\n"));

    // Grep is served from the search index; the root is indexed on demand.
    let found = c
        .repo
        .grep(pb::GrepRequest {
            repo: repo.into(),
            root: Some(pb::RootSpec {
                root: Some(pb::root_spec::Root::VersionHash(second.hash.clone())),
            }),
            pattern: "THERE".into(),
            case_insensitive: true,
            path_globs: vec!["*.md".into()],
            max_results: 10,
            context_lines: 1,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(found.hits.len(), 1, "{found:?}");
    assert_eq!(found.hits[0].path, "README.md");
    assert_eq!(found.hits[0].line_no, 2);
    assert_eq!(found.hits[0].before.len(), 1);
    assert!(!found.truncated);
    let none = c
        .repo
        .grep(pb::GrepRequest {
            repo: repo.into(),
            root: None,
            pattern: "THERE".into(),
            case_insensitive: false,
            path_globs: vec![],
            max_results: 0,
            context_lines: 0,
        })
        .await
        .unwrap()
        .into_inner();
    assert!(none.hits.is_empty());

    // Workspace flow: upload a blob, snapshot the tree it belongs to, and
    // turn the snapshot into a change.
    let big = vec![b'x'; 300];
    let put = c
        .repo
        .put_blob(tokio_stream::iter(vec![
            pb::PutBlobRequest {
                msg: Some(pb::put_blob_request::Msg::Header(pb::PutBlobHeader {
                    operation_id: Bytes::new(),
                    repo: repo.into(),
                    size: Some(big.len() as u64),
                    expected_hash: Some(Bytes::copy_from_slice(
                        ObjectHash::digest(ObjectKind::Blob, &big).as_bytes(),
                    )),
                })),
            },
            pb::PutBlobRequest {
                msg: Some(pb::put_blob_request::Msg::Data(Bytes::from(big.clone()))),
            },
        ]))
        .await
        .unwrap()
        .into_inner();
    let blob_ref = put.content.unwrap();
    let blob_hash = match &blob_ref.r#ref {
        Some(pb::content_ref::Ref::Blob(b)) => ObjectHash::from_slice(&b.hash).unwrap(),
        other => panic!("expected a stored blob, got {other:?}"),
    };

    let ws = c
        .workspace
        .create_workspace(pb::CreateWorkspaceRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            name: "scratch".into(),
            base: None,
            sparse: None,
        })
        .await
        .unwrap()
        .into_inner()
        .workspace
        .unwrap();
    assert_eq!(ws.base, first.hash);

    // The snapshot tree is built server-side through ApplyEdits (a draft
    // version whose root tree we reuse), as a client's tree builder would.
    let draft = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: Some(pb::EditBase {
                base: Some(pb::edit_base::Base::Root(trunk())),
            }),
            edits: vec![pb::Edit {
                path: "data.bin".into(),
                op: Some(pb::edit::Op::SetContent(pb::SetContent {
                    content: Some(blob_ref.clone()),
                    kind: pb::FileKind::Regular as i32,
                    copy_from: None,
                })),
            }],
            message: "draft".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner();
    let draft_root = c
        .repo
        .get_derived(pb::GetDerivedRequest {
            repo: repo.into(),
            kind: "root@1".into(),
            hash: draft.hash.clone(),
        })
        .await
        .unwrap()
        .into_inner()
        .data;
    // Derived records are postcard: a one-byte version then the 32-byte hash.
    let tree_hash = Bytes::copy_from_slice(&draft_root[draft_root.len() - 32..]);
    c.workspace
        .snapshot(pb::SnapshotRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            workspace: ws.id.clone(),
            tree_hash,
        })
        .await
        .unwrap();
    let new_change = c
        .workspace
        .new_change(pb::NewChangeRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            workspace: ws.id.clone(),
            message: "from workspace".into(),
            intent: None,
            provenance: None,
        })
        .await
        .unwrap()
        .into_inner();
    assert_eq!(new_change.change_id, ws.working_change);
    let version = new_change.version.unwrap();
    assert_eq!(version.parents, vec![first.hash.clone()]);
    assert!(version.file_changes.contains_key("data.bin"));
    let ops = c
        .workspace
        .list_operations(pb::ListOperationsRequest {
            repo: repo.into(),
            workspace: ws.id.clone(),
            limit: 0,
            before: None,
        })
        .await
        .unwrap()
        .into_inner()
        .operations;
    assert_eq!(ops.len(), 3);

    // Object protocol.
    let (status, headers, body) = http_get(
        server.http,
        &format!("/o/{repo}/blob/{}", blob_hash.to_hex()),
    )
    .await;
    assert_eq!(status, 200);
    assert_eq!(body, big);
    let etag = headers
        .iter()
        .find(|(k, _)| k == "etag")
        .map(|(_, v)| v.clone());
    assert_eq!(
        etag.as_deref(),
        Some(format!("\"{}\"", blob_hash.to_hex()).as_str())
    );
    assert!(headers
        .iter()
        .any(|(k, v)| k == "cache-control" && v.contains("immutable")));
    let (status, _, _) = http_get(server.http, "/healthz").await;
    assert_eq!(status, 200);
    let (status, _, _) = http_get(server.http, "/o/main/blob/zz").await;
    assert_eq!(status, 400);

    let me = c
        .admin
        .who_am_i(pb::WhoAmIRequest {})
        .await
        .unwrap()
        .into_inner();
    assert_eq!(me.principal.unwrap().name, "dev");
    let health = c
        .admin
        .health(pb::HealthRequest {})
        .await
        .unwrap()
        .into_inner();
    assert!(health.ok);

    server.stop().await;
}
