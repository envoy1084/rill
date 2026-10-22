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
    let version = callers.acl.version.clone();
    let at = callers.acl.at_version();
    let secret_hash = ObjectHash::digest(ObjectKind::Blob, &secret_bytes());
    let empty_tree = pb::RootSpec {
        root: Some(pb::root_spec::Root::TreeHash(Bytes::copy_from_slice(
            rill_types::Tree::empty_hash().as_bytes(),
        ))),
    };
    let probes: Vec<(&str, Vec<u8>)> = vec![
        (
            "RepoService/ListDir",
            pb::ListDirRequest {
                repo: "acl".into(),
                root: at.clone().into(),
                path: String::new(),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/ListDir",
            pb::ListDirRequest {
                repo: "acl".into(),
                root: at.clone().into(),
                path: "secret".into(),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/Log",
            pb::LogRequest {
                repo: "acl".into(),
                limit: 100,
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/Log",
            pb::LogRequest {
                repo: "acl".into(),
                path: Some("secret".into()),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/GetChangeVersion",
            pb::GetChangeVersionRequest {
                repo: "acl".into(),
                hash: version.clone(),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/GetChangeInfo",
            pb::GetChangeInfoRequest {
                repo: "acl".into(),
                hash: version.clone(),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/Diff",
            pb::DiffRequest {
                repo: "acl".into(),
                base: empty_tree.into(),
                target: at.clone().into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/Grep",
            pb::GrepRequest {
                repo: "acl".into(),
                root: at.clone().into(),
                pattern: CANARY.into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/ResolvePath",
            pb::ResolvePathRequest {
                repo: "acl".into(),
                root: at.clone().into(),
                path: "secret/s.txt".into(),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/PathHistory",
            pb::PathHistoryRequest {
                repo: "acl".into(),
                path: "secret/s.txt".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/Blame",
            pb::BlameRequest {
                repo: "acl".into(),
                path: "secret/s.txt".into(),
                ..Default::default()
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/WatchLog",
            pb::WatchLogRequest {
                repo: "acl".into(),
                after: Some(Bytes::new()),
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/TrunkRange",
            pb::TrunkRangeRequest {
                repo: "acl".into(),
                from: 1,
                to: 10,
            }
            .encode_to_vec(),
        ),
        (
            "RepoService/GetBlob",
            pb::GetBlobRequest {
                repo: "acl".into(),
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
            "RepoService/HasObjects",
            pb::HasObjectsRequest {
                repo: "acl".into(),
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
                repo: "acl".into(),
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
