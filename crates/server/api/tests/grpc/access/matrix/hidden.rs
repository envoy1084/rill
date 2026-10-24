//! A path-limited reader probing the directory it cannot read.

use buffa::Message;
use rill_types::{ObjectHash, ObjectKind};

use super::super::fixture::*;
use super::super::raw;
use super::callers::callers;
use super::leaks::assert_no_leak;
use super::roles::Role;
use crate::*;

/// Requests aimed at the hidden directory, by the reader of `pub/`: the
/// answers name nothing in `secret/`, whatever route they take.
#[tokio::test(flavor = "multi_thread")]
async fn a_path_reader_learns_nothing_of_the_hidden_directory() {
    let callers = callers().await;
    let channel = channel(&callers.acl.server.grpc).await;
    let token = callers
        .tokens
        .iter()
        .find(|(r, _)| *r == Role::PubReader)
        .and_then(|(_, t)| t.clone())
        .unwrap();
    let repo = || common::names::repo("acl");
    let version = common::names::version("acl", &callers.acl.version);
    let at = callers.acl.at_version();
    let secret_hash = ObjectHash::digest(ObjectKind::Blob, &secret_bytes());
    let empty_tree = format!("tree:{}", rill_types::Tree::empty_hash().to_hex());
    let probes: Vec<(&str, Vec<u8>)> = vec![
        (
            "ContentService/ListDirectory",
            pb::ListDirectoryRequest {
                repo: repo(),
                revision: at.clone(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/ListDirectory",
            pb::ListDirectoryRequest {
                repo: repo(),
                revision: at.clone(),
                path: "secret".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/ListHistory",
            pb::ListHistoryRequest {
                repo: repo(),
                page_size: 100,
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/ListHistory",
            pb::ListHistoryRequest {
                repo: repo(),
                path: "secret".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ChangeService/GetVersion",
            pb::GetVersionRequest {
                name: version.clone(),
                view: pb::VersionView::Full.into(),
            }
            .encode_to_vec(),
        ),
        (
            "ChangeService/GetVersion",
            pb::GetVersionRequest {
                name: version.clone(),
                view: pb::VersionView::Basic.into(),
            }
            .encode_to_vec(),
        ),
        (
            "ChangeService/BatchGetVersions",
            pb::BatchGetVersionsRequest {
                parent: repo(),
                names: vec![version.clone()],
                view: pb::VersionView::Full.into(),
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/Diff",
            pb::DiffRequest {
                repo: repo(),
                base: empty_tree,
                target: at.clone(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/Grep",
            pb::GrepRequest {
                repo: repo(),
                revision: at.clone(),
                pattern: CANARY.into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/GrepShards",
            pb::GrepShardsRequest {
                grep: Some(pb::GrepRequest {
                    repo: repo(),
                    revision: at.clone(),
                    pattern: CANARY.into(),
                    ..Default::default()
                })
                .into(),
                shards: (0..16).collect(),
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/Stat",
            pb::StatRequest {
                repo: repo(),
                revision: at.clone(),
                path: "secret/s.txt".into(),
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/ReadFile",
            pb::ReadFileRequest {
                repo: repo(),
                revision: at.clone(),
                path: "secret/s.txt".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/BatchReadFiles",
            pb::BatchReadFilesRequest {
                repo: repo(),
                revision: at.clone(),
                paths: vec!["pub/a.txt".into(), "secret/s.txt".into()],
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/ListPathHistory",
            pb::ListPathHistoryRequest {
                repo: repo(),
                path: "secret/s.txt".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/Blame",
            pb::BlameRequest {
                repo: repo(),
                path: "secret/s.txt".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/WatchVersions",
            pb::WatchVersionsRequest {
                parent: repo(),
                after: Some(Bytes::new()),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/ListTrunkPositions",
            pb::ListTrunkPositionsRequest {
                parent: repo(),
                filter: "position >= 1 AND position <= 10".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/ReadBlob",
            pb::ReadBlobRequest {
                repo: repo(),
                content: (pb::ContentRef {
                    r#ref: Some(pb::content_ref::Ref::Blob(pb::BlobRef {
                        hash: Bytes::copy_from_slice(secret_hash.as_bytes()),
                        size: secret_bytes().len() as u64,
                    })),
                })
                .into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "ContentService/BatchCheckObjects",
            pb::BatchCheckObjectsRequest {
                repo: repo(),
                objects: vec![pb::ObjectRef {
                    kind: pb::ObjectKind::Blob.into(),
                    hash: Bytes::copy_from_slice(secret_hash.as_bytes()),
                }],
            }
            .encode_to_vec(),
        ),
        (
            "AccessService/LookupPaths",
            pb::LookupPathsRequest {
                parent: repo(),
                relation: "reader".into(),
                subject: None,
            }
            .encode_to_vec(),
        ),
    ];
    let methods = raw::methods();
    for (name, body) in probes {
        let method = methods
            .iter()
            .find(|m| m.name == format!("rill.v1.{name}"))
            .unwrap();
        let outcome = raw::call(channel.clone(), method, Bytes::from(body), Some(&token)).await;
        assert_no_leak(name, &outcome);
    }
    let http = reqwest::Client::new()
        .get(format!(
            "http://{}/o/acl/blob/{}",
            callers.acl.server.http,
            secret_hash.to_hex()
        ))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(http.status(), 403);
    callers.acl.server.stop().await;
}
