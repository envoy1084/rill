//! The event log over gRPC: every change shows up once, in order, and a
//! watch resumes after a cursor.

use super::*;

async fn next_event(
    watch: &mut futures::stream::BoxStream<'static, Result<pb::EventEnvelope, ConnectError>>,
) -> (Bytes, pb::event::Kind) {
    let envelope = tokio::time::timeout(Duration::from_secs(5), watch.next())
        .await
        .expect("an event")
        .unwrap()
        .unwrap();
    (envelope.cursor, envelope.event.unwrap().kind.unwrap())
}

#[tokio::test]
async fn watch_events_sees_each_change_once_and_resumes() {
    use pb::event::Kind;
    let server = TestServer::start(dev_config()).await;
    let mut c = clients(&server, None).await;
    let repo = "events";
    c.repo
        .create_repo(pb::CreateRepoRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            description: None,
            windows_safe: false,
        })
        .await
        .unwrap();
    let watch = |after: Option<Bytes>, kinds: Vec<String>| pb::WatchEventsRequest {
        repo: repo.into(),
        after,
        kinds,
        change_id: None,
        ref_prefix: None,
    };
    let mut all = c
        .repo
        .watch_events(watch(Some(Bytes::new()), vec![]))
        .await
        .unwrap()
        .messages();
    let mut refs = c
        .repo
        .watch_refs(pb::WatchRefsRequest {
            repo: repo.into(),
            prefix: None,
        })
        .await
        .unwrap()
        .messages();

    let created = c
        .repo
        .apply_edits(pb::ApplyEditsRequest {
            operation_id: Bytes::new(),
            repo: repo.into(),
            base: None.into(),
            edits: vec![set_edit("a.txt", b"a\n")],
            message: "a".into(),
            intent: None,
            provenance: None.into(),
            change_id: None,
        })
        .await
        .unwrap()
        .into_owned();
    let change_id = created.version.unwrap().change_id;
    land(&mut c, repo, change_id.clone()).await;

    let mut seen = Vec::new();
    loop {
        let (cursor, kind) = next_event(&mut all).await;
        let done = matches!(&kind, Kind::Landed(l) if l.change_id == change_id);
        seen.push((cursor, kind));
        if done {
            break;
        }
    }
    assert!(matches!(seen[0].1, Kind::RepoCreated(_)));
    let stored = seen
        .iter()
        .filter(|(_, k)| matches!(k, Kind::VersionStored(v) if v.change_id == change_id))
        .count();
    assert_eq!(stored, 1, "one event per stored version");
    assert!(seen.windows(2).all(|w| w[0].0 < w[1].0), "cursors increase");

    let trunk = tokio::time::timeout(Duration::from_secs(5), refs.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(trunk.name, "trunk");
    assert!(trunk.old.is_none() && trunk.new.is_some());

    let mut resumed = c
        .repo
        .watch_events(watch(Some(seen[0].0.clone()), vec![]))
        .await
        .unwrap()
        .messages();
    assert_eq!(next_event(&mut resumed).await, seen[1].clone());

    let mut landed_only = c
        .repo
        .watch_events(watch(Some(Bytes::new()), vec!["landed".into()]))
        .await
        .unwrap()
        .messages();
    let (_, kind) = next_event(&mut landed_only).await;
    assert!(matches!(kind, Kind::Landed(l) if l.seq == 1));
    server.stop().await;
}
