//! The event log on every backend: one event per committed write.

use std::time::Duration;

use rill_store::Stores;
use rill_types::{Principal, RefName, Timestamp};

use crate::repo::cv;

pub async fn scenario(name: &str, stores: &Stores) {
    let alice = Principal::human("alice");
    let config = stores.create_repo("events", "").await.unwrap();
    let repo = stores.repo(config.id);

    let root = repo.put_change_version(&cv("root", vec![])).await.unwrap();
    repo.put_change_version(&cv("root", vec![])).await.unwrap();
    let feature = RefName::new("feature").unwrap();
    repo.update_ref(&feature, None, Some(root), &alice)
        .await
        .unwrap();
    assert!(repo
        .update_ref(&feature, None, Some(root), &alice)
        .await
        .unwrap_err()
        .is_conflict());
    repo.add_relation("repo", "reader", "bob", &alice)
        .await
        .unwrap();
    repo.remove_relation("repo", "reader", "bob", &alice)
        .await
        .unwrap();
    repo.land_on_trunk(None, root).await.unwrap();

    let events = repo.events_after(None, 100).await.unwrap();
    let kinds: Vec<_> = events.iter().map(|(_, e)| e.kind.name()).collect();
    assert_eq!(
        kinds,
        [
            "repo_created",
            "version_stored",
            "version_stored",
            "ref_moved",
            "relation_changed",
            "relation_changed",
            "landed",
        ],
        "{name}: a failed write records nothing"
    );
    assert_eq!(
        repo.event_count().await.unwrap(),
        events.len() as i64,
        "{name}"
    );
    let rest = repo.events_after(Some(events[2].0), 100).await.unwrap();
    assert_eq!(
        rest.len(),
        events.len() - 3,
        "{name}: resumes after a cursor"
    );

    let seen = repo.event_count().await.unwrap();
    assert_eq!(
        repo.wait_for_events(seen - 1, Duration::from_secs(5))
            .await
            .unwrap(),
        seen,
        "{name}: a stale count returns at once"
    );
    let waiter = {
        let repo = repo.clone();
        tokio::spawn(async move { repo.wait_for_events(seen, Duration::from_secs(5)).await })
    };
    tokio::time::sleep(Duration::from_millis(50)).await;
    repo.add_relation("repo", "reader", "carol", &alice)
        .await
        .unwrap();
    assert_eq!(
        waiter.await.unwrap().unwrap(),
        seen + 1,
        "{name}: wakes on a write"
    );

    let far = Timestamp::from_millis(Timestamp::now().millis() + 60_000);
    assert_eq!(repo.sweep_events(far, 3).await.unwrap(), 3, "{name}");
    assert_eq!(
        repo.events_after(None, 100).await.unwrap().len(),
        events.len() + 1 - 3,
        "{name}: sweeps oldest first"
    );
    assert_eq!(
        repo.event_count().await.unwrap(),
        seen + 1,
        "{name}: the count stays"
    );
}
