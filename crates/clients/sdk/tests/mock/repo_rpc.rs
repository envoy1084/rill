//! RepoClient against the mock server: streaming, conversion and auth.

use crate::common::{fake_hash, sample_info, Harness, MockFile, MockRepo};
use bytes::Bytes;
use futures::StreamExt;
use rill_sdk::repo::PUT_BLOB_CHUNK_SIZE;
use rill_sdk::{ApplyOptions, Edit, EditBase, LogStart, Root};
use rill_types::{
    ChangeId, ContentRef, EntryKind, FileChange, FileKind, ObjectHash, ObjectKind, RepoPathBuf,
    TrunkSeq,
};

fn path(s: &str) -> RepoPathBuf {
    RepoPathBuf::new(s).unwrap()
}

#[tokio::test]
async fn read_file_reassembles_streamed_chunks() {
    let mock = MockRepo::new();
    let big: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
    mock.set_tree(
        "trunk",
        vec![
            ("small.txt", MockFile::regular("hello")),
            ("dir/big.bin", MockFile::regular(big.clone())),
        ],
    );
    let h = Harness::with_repo(mock).await;
    let repo = h.client.repo("main");

    let small = repo
        .read_file(&Root::trunk(), &path("small.txt"))
        .await
        .unwrap();
    assert_eq!(&small[..], b"hello");

    let got = repo
        .read_file(&Root::trunk(), &path("dir/big.bin"))
        .await
        .unwrap();
    assert_eq!(got.len(), big.len());
    assert_eq!(&got[..], &big[..]);

    let err = repo
        .read_file(&Root::trunk(), &path("nope"))
        .await
        .unwrap_err();
    assert!(err.is_not_found());

    let listing = repo
        .list_dir(&Root::trunk(), &RepoPathBuf::root())
        .await
        .unwrap();
    let names: Vec<_> = listing.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["dir", "small.txt"]);
    assert!(matches!(listing[0].1, EntryKind::Dir { .. }));
}

#[tokio::test]
async fn put_blob_streams_in_chunks() {
    let h = Harness::with_repo(MockRepo::new()).await;
    let repo = h.client.repo("main");
    let data = Bytes::from(vec![7u8; PUT_BLOB_CHUNK_SIZE * 2 + 10]);

    let content = repo.put_blob(data.clone()).await.unwrap();
    match content {
        ContentRef::Blob { hash, size } => {
            assert_eq!(size, data.len() as u64);
            assert_eq!(hash, ObjectHash::digest(ObjectKind::Blob, &data));
        }
        other => panic!("unexpected content ref {other:?}"),
    }
    // One header plus three data messages.
    assert_eq!(h.repo.put_requests.lock().unwrap().as_slice(), &[4]);

    let back = repo.get_blob(&content).await.unwrap();
    assert_eq!(back, data);

    let small = repo
        .store_content(Bytes::from_static(b"tiny"))
        .await
        .unwrap();
    assert!(small.is_inline());
}

#[tokio::test]
async fn log_converts_entries() {
    let mock = MockRepo::new();
    *mock.log.lock().unwrap() = vec![sample_info("a", Some(3)), sample_info("b", None)];
    let h = Harness::with_repo(mock).await;
    let repo = h.client.repo("main");

    let entries = repo.log(LogStart::Trunk, 10, None).await.unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].hash, fake_hash("a"));
    assert_eq!(entries[0].change_id, ChangeId::derived("test", "a"));
    assert_eq!(entries[0].trunk_seq, Some(TrunkSeq(3)));
    assert_eq!(entries[0].summary(), "change a");
    assert_eq!(entries[0].author.name, "alice");
    assert_eq!(entries[1].trunk_seq, None);

    let one = repo.log(LogStart::Seq(TrunkSeq(3)), 1, None).await.unwrap();
    assert_eq!(one.len(), 1);
}

#[tokio::test]
async fn apply_edits_round_trips_types() {
    let mock = MockRepo::new();
    let head = fake_hash("head");
    mock.set_trunk(5, head);
    let h = Harness::with_repo(mock).await;
    let repo = h.client.repo("main");

    let big = Bytes::from(vec![1u8; 5000]);
    let edits = vec![
        Edit::set(path("a.txt"), "hi"),
        Edit::Set {
            path: path("bin/tool"),
            bytes: big.clone(),
            kind: FileKind::Executable,
        },
        Edit::delete(path("old.txt")),
        Edit::Patch {
            path: path("p.txt"),
            unified: "--- a\n+++ b\n".into(),
        },
    ];
    let change_id = ChangeId::new();
    let result = repo
        .apply_edits(
            EditBase::Root(Root::trunk()),
            edits,
            ApplyOptions::message("edit").amending(change_id),
        )
        .await
        .unwrap();

    assert_eq!(result.version.change_id, change_id);
    assert_eq!(result.version.parents, vec![head]);
    assert_eq!(result.version.message, "edit");
    assert_eq!(result.version.file_changes.len(), 4);
    match &result.version.file_changes[&path("a.txt")] {
        FileChange::Set { content, kind, .. } => {
            assert_eq!(content, &ContentRef::inline(Bytes::from_static(b"hi")));
            assert_eq!(*kind, FileKind::Regular);
        }
        other => panic!("{other:?}"),
    }
    match &result.version.file_changes[&path("bin/tool")] {
        FileChange::Set { content, kind, .. } => {
            assert!(matches!(content, ContentRef::Blob { size: 5000, .. }));
            assert_eq!(*kind, FileKind::Executable);
            assert_eq!(repo.get_blob(content).await.unwrap(), big);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        result.version.file_changes[&path("old.txt")],
        FileChange::Delete
    );

    let sent = h.repo.applied.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert!(uuid::Uuid::parse_str(&sent[0].request_id).is_ok());
    assert_eq!(sent[0].parent, "orgs/default/repos/main");
}

#[tokio::test]
async fn bearer_token_is_sent_as_metadata() {
    let h = Harness::start(
        MockRepo::new(),
        crate::common::MockReview::default(),
        Some("s3cret"),
    )
    .await;
    let repo = h.client.repo("main");
    let info = repo.info().await.unwrap();
    assert_eq!(info.name, "main");
    let headers = h.repo.auth_headers.lock().unwrap();
    assert_eq!(headers.as_slice(), &[Some("Bearer s3cret".to_string())]);
}

#[tokio::test]
async fn no_token_means_no_header_and_status_codes_survive() {
    let h = Harness::with_repo(MockRepo::new()).await;
    h.client.repo("main").trunk_head().await.unwrap();
    assert_eq!(h.repo.auth_headers.lock().unwrap().as_slice(), &[None]);

    let err = h.client.repo("missing").info().await.unwrap_err();
    assert_eq!(err.code(), Some(rill_sdk::Code::NotFound));

    let err = h.client.repo("main").list_refs(None).await.unwrap_err();
    assert_eq!(err.code(), Some(rill_sdk::Code::Unimplemented));
}

#[tokio::test]
async fn watch_trunk_yields_converted_events() {
    let h = Harness::with_repo(MockRepo::new()).await;
    let mut stream = h.client.repo("main").watch_trunk(None).await.unwrap();
    let mut seqs = Vec::new();
    while let Some(ev) = stream.next().await {
        seqs.push(ev.unwrap().seq);
    }
    assert_eq!(seqs, vec![TrunkSeq(1), TrunkSeq(2), TrunkSeq(3)]);
}

#[tokio::test]
async fn writes_retry_through_unavailable_servers_with_one_request_id() {
    let mock = MockRepo::new();
    mock.set_trunk(1, fake_hash("head"));
    mock.unavailable
        .store(2, std::sync::atomic::Ordering::SeqCst);
    let h = Harness::with_repo(mock).await;
    let repo = h.client.repo("main");
    repo.apply_edits(
        EditBase::Root(Root::trunk()),
        vec![Edit::set(path("a.txt"), "hi")],
        ApplyOptions::message("retried"),
    )
    .await
    .unwrap();
    let applied = h.repo.applied.lock().unwrap();
    assert_eq!(applied.len(), 3, "two failures, then success");
    assert!(
        applied
            .iter()
            .all(|r| r.request_id == applied[0].request_id && !r.request_id.is_empty()),
        "every attempt is the same operation"
    );
}

#[tokio::test]
async fn a_refusal_that_asks_for_a_wait_is_retried_after_it() {
    let mock = MockRepo::new();
    mock.set_trunk(1, fake_hash("head"));
    mock.rate_limited
        .store(1, std::sync::atomic::Ordering::SeqCst);
    let h = Harness::with_repo(mock).await;
    let started = std::time::Instant::now();
    h.client
        .repo("main")
        .apply_edits(
            EditBase::Root(Root::trunk()),
            vec![Edit::set(path("a.txt"), "hi")],
            ApplyOptions::message("waited"),
        )
        .await
        .unwrap();
    assert_eq!(h.repo.applied.lock().unwrap().len(), 2);
    assert!(started.elapsed() >= std::time::Duration::from_secs(1));
}

#[tokio::test]
async fn a_refusal_without_a_wait_is_not_retried() {
    let mock = MockRepo::new();
    mock.set_trunk(1, fake_hash("head"));
    mock.too_large.store(1, std::sync::atomic::Ordering::SeqCst);
    let h = Harness::with_repo(mock).await;
    let err = h
        .client
        .repo("main")
        .apply_edits(
            EditBase::Root(Root::trunk()),
            vec![Edit::set(path("a.txt"), "hi")],
            ApplyOptions::message("too big"),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code(), Some(rill_sdk::Code::ResourceExhausted));
    assert_eq!(h.repo.applied.lock().unwrap().len(), 1);
}
