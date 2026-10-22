//! The event log over gRPC: every change shows up once, in order, and a
//! watch resumes after a cursor.

use super::*;

async fn next_event(
    watch: &mut futures::stream::BoxStream<'static, Result<pb::WatchEventsResponse, ConnectError>>,
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
            parent: common::names::org("default"),
            repo_id: repo.into(),
            ..Default::default()
        })
        .await
        .unwrap();
    let watch = |after: Option<Bytes>, kinds: Vec<String>| pb::WatchEventsRequest {
        parent: common::names::repo(repo),
        after,
        kinds,
        change_id: None,
        ref_prefix: None,
    };
    let mut all = c
        .events
        .watch_events(watch(Some(Bytes::new()), vec![]))
        .await
        .unwrap()
        .messages();
    let mut refs = c
        .repo
        .watch_refs(pb::WatchRefsRequest {
            parent: common::names::repo(repo),
            prefix: String::new(),
        })
        .await
        .unwrap()
        .messages();

    let created = c
        .changes
        .apply_edits(pb::ApplyEditsRequest {
            parent: common::names::repo(repo),
            edits: vec![set_edit("a.txt", b"a\n")],
            message: "a".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .into_owned();
    let change_id = created.info.unwrap().change_id;
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
    assert_eq!(trunk.r#ref, "trunk");
    assert!(trunk.old.is_none() && trunk.new.is_some());

    let mut resumed = c
        .events
        .watch_events(watch(Some(seen[0].0.clone()), vec![]))
        .await
        .unwrap()
        .messages();
    assert_eq!(next_event(&mut resumed).await, seen[1].clone());

    let mut landed_only = c
        .events
        .watch_events(watch(Some(Bytes::new()), vec!["landed".into()]))
        .await
        .unwrap()
        .messages();
    let (_, kind) = next_event(&mut landed_only).await;
    assert!(matches!(kind, Kind::Landed(l) if l.seq == 1));
    server.stop().await;
}
