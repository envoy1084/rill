//! The regional cache in front of a real origin: it serves what a grant
//! names, from disk after the first fetch, and nothing else.

mod common;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use bytes::Bytes;
use common::secure::SecureServer;
use rill_cache::{Cache, CacheConfig};
use rill_proto::rill::v1 as pb;
use rill_sdk::{ApplyOptions, Edit, EditBase, Root};
use rill_types::{ObjectHash, ObjectKind, Principal, RepoPathBuf};
use tokio_util::sync::CancellationToken;

fn path(p: &str) -> RepoPathBuf {
    RepoPathBuf::new(p).unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn the_cache_serves_what_grants_name_and_fetches_each_object_once() {
    let server = SecureServer::start().await;
    let (ann, bob) = (Principal::human("ann"), Principal::human("bob"));
    let owner = server.client(&ann).await;
    owner.create_repo("acl", None).await.unwrap();
    let first = owner
        .repo_svc()
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Default::default(),
            repo: "acl".into(),
            base: None,
            edits: Vec::new(),
            message: "start".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner()
        .hash;
    let files = [
        ("pub/a.txt", "a\n".repeat(400)),
        ("pub/b.txt", "b\n".repeat(400)),
        ("secret/s.txt", "s\n".repeat(400)),
    ];
    let version = owner
        .repo("acl")
        .apply_edits(
            EditBase::Root(Root::Version(ObjectHash::from_slice(&first).unwrap())),
            files
                .iter()
                .map(|(p, c)| Edit::set(path(p), c.clone()))
                .collect(),
            ApplyOptions {
                message: "seed".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .hash;
    owner
        .access()
        .grant(&rill_sdk::Relation::new(
            "acl",
            "pub",
            "reader",
            format!("principal:{}", bob.id),
        ))
        .await
        .unwrap();

    let dir = tempfile::tempdir().unwrap();
    let mut config = CacheConfig::new(format!("http://{}", server.http), dir.path(), 1 << 20);
    config.listen = "127.0.0.1:0".parse().unwrap();
    let cache = Cache::start(&config).await.unwrap();
    let listener = tokio::net::TcpListener::bind(config.listen).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let stop = CancellationToken::new();
    tokio::spawn({
        let (cache, config, stop) = (cache.clone(), config.clone(), stop.clone());
        async move { rill_cache::serve_on(cache, listener, &config, stop).await }
    });

    // Bob lists pub/ at the origin and gets a grant for what is there.
    let reader = server.client(&bob).await;
    let listing = reader
        .repo_svc()
        .list_dir(pb::ListDirRequest {
            repo: "acl".into(),
            root: Some(Root::Version(version).to_proto()),
            path: "pub".into(),
        })
        .await
        .unwrap()
        .into_inner();
    let grant = listing.read_grant;
    let token = server.token(&bob);
    let http = reqwest::Client::new();
    let fetch = |hash: ObjectHash, grant: String| {
        let (http, token) = (http.clone(), token.clone());
        async move {
            let response = http
                .get(format!("http://{addr}/o/acl/blob/{}", hash.to_hex()))
                .bearer_auth(token)
                .header("x-rill-grant", grant)
                .send()
                .await
                .unwrap();
            (response.status().as_u16(), response.bytes().await.unwrap())
        }
    };
    let blob = |content: &str| ObjectHash::digest(ObjectKind::Blob, content.as_bytes());
    let (a, b, s) = (blob(&files[0].1), blob(&files[1].1), blob(&files[2].1));

    let (status, body) = fetch(a, grant.clone()).await;
    assert_eq!((status, body), (200, Bytes::from(files[0].1.clone())));
    assert_eq!(fetch(a, grant.clone()).await.0, 200);
    let stats = cache.stats();
    assert_eq!(
        stats.origin_fetches.load(Ordering::Relaxed),
        1,
        "the second read is a hit"
    );
    assert_eq!(stats.hits.load(Ordering::Relaxed), 1);

    // A secret hash, a forged grant and no grant are refused without
    // asking the origin.
    assert_eq!(fetch(s, grant.clone()).await.0, 403);
    let mut forged = grant.clone().into_bytes();
    let last = forged.len() - 3;
    forged[last] = if forged[last] == b'A' { b'B' } else { b'A' };
    assert_eq!(fetch(a, String::from_utf8(forged).unwrap()).await.0, 403);
    assert_eq!(fetch(a, String::new()).await.0, 403);
    assert_eq!(stats.origin_fetches.load(Ordering::Relaxed), 1);

    // Fifty clients missing on one object cost the origin one fetch.
    let misses: Vec<_> = (0..50)
        .map(|_| tokio::spawn(fetch(b, grant.clone())))
        .collect();
    for miss in misses {
        assert_eq!(miss.await.unwrap().0, 200);
    }
    assert_eq!(stats.origin_fetches.load(Ordering::Relaxed), 2);
    assert!(stats.coalesced.load(Ordering::Relaxed) + stats.hits.load(Ordering::Relaxed) >= 50);

    // Objects on disk survive a restart of the cache.
    let reopened = Cache::start(&config).await.unwrap();
    assert_eq!(reopened.disk().len(), 2);
    let _: Arc<Cache> = reopened;
    stop.cancel();
    server.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_redacted_object_is_dropped_from_the_cache_and_never_served() {
    let server = SecureServer::start_with_admins(&["root", "sec"]).await;
    let (root, sec) = (Principal::human("root"), Principal::human("sec"));
    let owner = server.client(&root).await;
    owner.create_repo("app", None).await.unwrap();
    let leaked = "customer: alice\n".repeat(100);
    let first = owner
        .repo_svc()
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Default::default(),
            repo: "app".into(),
            base: None,
            edits: Vec::new(),
            message: "start".into(),
            intent: None,
            provenance: None,
            change_id: None,
        })
        .await
        .unwrap()
        .into_inner()
        .hash;
    let version = owner
        .repo("app")
        .apply_edits(
            EditBase::Root(Root::Version(ObjectHash::from_slice(&first).unwrap())),
            vec![Edit::set(path("data/c.txt"), leaked.clone())],
            ApplyOptions {
                message: "oops".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .hash;
    let grant = owner
        .repo_svc()
        .list_dir(pb::ListDirRequest {
            repo: "app".into(),
            root: Some(Root::Version(version).to_proto()),
            path: "data".into(),
        })
        .await
        .unwrap()
        .into_inner()
        .read_grant;

    let dir = tempfile::tempdir().unwrap();
    let mut config = CacheConfig::new(format!("http://{}", server.http), dir.path(), 1 << 20);
    config.listen = "127.0.0.1:0".parse().unwrap();
    config.redaction_poll = std::time::Duration::from_millis(50);
    let cache = Cache::start(&config).await.unwrap();
    let listener = tokio::net::TcpListener::bind(config.listen).await.unwrap();
    let addr = listener.local_addr().unwrap();
    let stop = CancellationToken::new();
    tokio::spawn({
        let (cache, config, stop) = (cache.clone(), config.clone(), stop.clone());
        async move { rill_cache::serve_on(cache, listener, &config, stop).await }
    });
    let hash = ObjectHash::digest(ObjectKind::Blob, leaked.as_bytes());
    let token = server.token(&root);
    let fetch = || {
        let (token, grant) = (token.clone(), grant.clone());
        async move {
            reqwest::Client::new()
                .get(format!("http://{addr}/o/app/blob/{}", hash.to_hex()))
                .bearer_auth(token)
                .header("x-rill-grant", grant)
                .send()
                .await
                .unwrap()
                .status()
                .as_u16()
        }
    };
    assert_eq!(fetch().await, 200);
    assert_eq!(cache.disk().len(), 1);

    let request = owner
        .protection_svc()
        .request_redaction(pb::RequestRedactionRequest {
            operation_id: Default::default(),
            org: "default".into(),
            kind: ObjectKind::Blob as i32,
            hash: hash.as_bytes().to_vec().into(),
            reason: "customer data".into(),
        })
        .await
        .unwrap()
        .into_inner()
        .redaction
        .unwrap();
    server
        .client(&sec)
        .await
        .protection_svc()
        .approve_redaction(pb::ApproveRedactionRequest {
            operation_id: Default::default(),
            org: "default".into(),
            id: request.id,
        })
        .await
        .unwrap();

    let mut status = 0;
    for _ in 0..100 {
        status = fetch().await;
        if status == 451 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(status, 451, "the cache stops serving it");
    assert_eq!(cache.disk().len(), 0, "and drops its copy");
    stop.cancel();
    server.stop();
}
