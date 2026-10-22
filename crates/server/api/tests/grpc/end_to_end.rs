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
        metadata: rill_runtime::config::MetadataConfig::Fdb {
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
    scenario(&server.grpc, server.http).await;
    server.stop().await;
}

/// One pass through every service against the server (or servers) at
/// `endpoint` and `http`.
pub(super) async fn scenario(endpoint: &str, http: std::net::SocketAddr) {
    let mut c = clients_at(endpoint, None).await;
    let repo = "main";
    let repo_name = common::names::repo(repo);

    let created = c
        .repo
        .create_repo(pb::CreateRepoRequest {
            request_id: String::new(),
            parent: common::names::org("default"),
            repo_id: repo.into(),
            repo: pb::Repo {
                description: Some("smoke".into()),
                ..Default::default()
            }
            .into(),
        })
        .await
        .unwrap()
        .into_owned();
    let created = created.repo.unwrap();
    assert_eq!(created.name, repo_name);
    assert_eq!(created.repo_id, "main");

    // Watch trunk before anything lands so the event is observed live.
    let mut trunk_watch = c
        .repo
        .watch_trunk(pb::WatchTrunkRequest {
            parent: repo_name.clone(),
            after_position: Some(0),
        })
        .await
        .unwrap()
        .messages();

    let first = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: repo_name.clone(),
            edits: vec![
                set_edit("README.md", b"hello\nworld\n"),
                set_edit("src/lib.rs", b"fn a() {}\n"),
            ],
            message: "initial".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    let first_info = first.info.unwrap();
    let change_id = first_info.change_id.clone();
    assert!(first.version.unwrap().parents.is_empty());

    let seq = land(&mut c, repo, change_id.clone()).await;
    assert_eq!(seq, 1);

    let landed = tokio::time::timeout(Duration::from_secs(5), trunk_watch.next())
        .await
        .expect("trunk event")
        .unwrap()
        .unwrap()
        .landing
        .unwrap();
    assert_eq!(landed.position, 1);
    assert_eq!(landed.change_id, change_id);

    let listing = c
        .content
        .list_directory(pb::ListDirectoryRequest {
            repo: repo_name.clone(),
            revision: trunk(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    let names: Vec<_> = listing.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["README.md", "src"]);

    // A second change on top of trunk, amended through a patch.
    let second = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: repo_name.clone(),
            base: common::trunk_base().into(),
            edits: vec![pb::Edit {
                path: "README.md".into(),
                op: Some(pb::edit::Op::Patch(
                    "--- a/README.md\n+++ b/README.md\n@@ -1,2 +1,2 @@\n hello\n-world\n+there\n"
                        .into(),
                )),
            }],
            message: "patch readme".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap();
    let second_rev = hex::encode(&second.hash);

    let diff = c
        .content
        .diff(pb::DiffRequest {
            repo: repo_name.clone(),
            base: trunk(),
            target: second_rev.clone(),
            format: pb::DiffFormat::Structured.into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(diff.files.len(), 1);
    assert_eq!(diff.files[0].path, "README.md");
    let hunk = &diff.files[0].hunks[0];
    let ops: Vec<i32> = hunk.lines.iter().map(|l| l.op.to_i32()).collect();
    assert!(ops.contains(&(pb::diff_line::Op::Delete as i32)));
    assert!(ops.contains(&(pb::diff_line::Op::Insert as i32)));

    let blame = c
        .content
        .blame(pb::BlameRequest {
            repo: repo_name.clone(),
            revision: second_rev.clone(),
            path: "README.md".into(),
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(blame.lines.len(), 2);
    assert_eq!(
        blame.lines[0].version,
        common::names::version(repo, &first_info.hash)
    );
    assert_eq!(blame.lines[0].trunk_position, Some(1));
    assert_eq!(
        blame.lines[1].version,
        common::names::version(repo, &second.hash)
    );
    assert_eq!(blame.lines[1].text, Bytes::from_static(b"there\n"));

    // Grep is served from the search index; the root is indexed on demand.
    let found = c
        .content
        .grep(pb::GrepRequest {
            repo: repo_name.clone(),
            revision: second_rev.clone(),
            pattern: "THERE".into(),
            case_insensitive: true,
            path_globs: vec!["*.md".into()],
            max_results: 10,
            context_lines: 1,
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(found.hits.len(), 1, "{found:?}");
    assert_eq!(found.hits[0].path, "README.md");
    assert_eq!(found.hits[0].line_no, 2);
    assert_eq!(found.hits[0].before.len(), 1);
    assert!(!found.truncated);
    let none = c
        .content
        .grep(pb::GrepRequest {
            repo: repo_name.clone(),
            revision: trunk(),
            pattern: "THERE".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert!(none.hits.is_empty());

    // Workspace flow: upload a blob, snapshot the tree it belongs to, and
    // turn the snapshot into a change.
    let big = vec![b'x'; 300];
    let put = c
        .content
        .write_blob(tokio_stream::iter(vec![
            pb::WriteBlobRequest {
                msg: Some(pb::write_blob_request::Msg::Header(pb::WriteBlobHeader {
                    request_id: String::new(),
                    repo: repo_name.clone(),
                    size: Some(big.len() as u64),
                    expected_hash: Some(Bytes::copy_from_slice(
                        ObjectHash::digest(ObjectKind::Blob, &big).as_bytes(),
                    )),
                })),
            },
            pb::WriteBlobRequest {
                msg: Some(pb::write_blob_request::Msg::Data(Bytes::from(big.clone()))),
            },
        ]))
        .await
        .unwrap()
        .into_owned();
    let blob_ref = put.content.unwrap();
    let blob_hash = match &blob_ref.r#ref {
        Some(pb::content_ref::Ref::Blob(b)) => ObjectHash::from_slice(&b.hash).unwrap(),
        other => panic!("expected a stored blob, got {other:?}"),
    };

    let ws = c
        .workspace
        .create_workspace(pb::CreateWorkspaceRequest {
            request_id: String::new(),
            parent: repo_name.clone(),
            workspace: pb::Workspace {
                display_name: "scratch".into(),
                ..Default::default()
            }
            .into(),
            base: String::new(),
        })
        .await
        .unwrap()
        .into_owned()
        .workspace
        .unwrap();
    assert_eq!(ws.base, first_info.hash);

    // The snapshot tree is built server-side through ApplyEdits (a draft
    // version whose root tree we reuse), as a client's tree builder would.
    let draft = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: repo_name.clone(),
            base: common::trunk_base().into(),
            edits: vec![pb::Edit {
                path: "data.bin".into(),
                op: Some(pb::edit::Op::SetContent(pb::SetContent {
                    content: blob_ref.clone().into(),
                    kind: pb::FileKind::Regular.into(),
                    copy_from: None.into(),
                })),
            }],
            message: "draft".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .info
        .unwrap();
    let draft_root = c
        .repo
        .get_derived_value(pb::GetDerivedValueRequest {
            repo: repo_name.clone(),
            kind: "root@1".into(),
            hash: draft.hash.clone(),
        })
        .await
        .unwrap()
        .into_owned()
        .data;
    // Derived records are postcard: a one-byte version then the 32-byte hash.
    let tree_hash = Bytes::copy_from_slice(&draft_root[draft_root.len() - 32..]);
    c.workspace
        .snapshot_workspace(pb::SnapshotWorkspaceRequest {
            request_id: String::new(),
            name: ws.name.clone(),
            tree_hash,
        })
        .await
        .unwrap();
    let new_change = c
        .workspace
        .start_change(pb::StartChangeRequest {
            name: ws.name.clone(),
            message: "from workspace".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    assert_eq!(
        new_change.change,
        common::names::change(repo, &ws.working_change)
    );
    let version = new_change.version.unwrap();
    assert_eq!(version.parents, vec![first_info.hash.clone()]);
    assert!(version.file_changes.contains_key("data.bin"));
    let ops = c
        .workspace
        .list_workspace_operations(pb::ListWorkspaceOperationsRequest {
            parent: ws.name.clone(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned()
        .operations;
    assert_eq!(ops.len(), 3);

    // Object protocol.
    let (status, headers, body) =
        http_get(http, &format!("/o/{repo}/blob/{}", blob_hash.to_hex())).await;
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
    let (status, _, _) = http_get(http, "/livez").await;
    assert_eq!(status, 200);
    let (status, _, _) = http_get(http, "/o/main/blob/zz").await;
    assert_eq!(status, 400);

    let me = c
        .identity
        .get_caller(pb::GetCallerRequest {})
        .await
        .unwrap()
        .into_owned();
    assert_eq!(me.principal.unwrap().name, "dev");
    let health = c
        .admin
        .get_health(pb::GetHealthRequest {})
        .await
        .unwrap()
        .into_owned();
    assert!(health.ok);
}
