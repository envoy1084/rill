//! Typed repository access on every backend.

use std::collections::BTreeMap;

use rill_store::Stores;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, Principal, Provenance, Timestamp, TrunkSeq,
};

pub fn cv(msg: &str, parents: Vec<ChangeVersionHash>) -> ChangeVersion {
    ChangeVersion {
        change_id: ChangeId::new(),
        parents,
        predecessor: None,
        subtree_changes: BTreeMap::new(),
        file_changes: BTreeMap::new(),
        author: Principal::human("a"),
        provenance: Provenance::default(),
        intent: None,
        message: msg.into(),
        created_at: Timestamp::from_millis(1),
        extra: BTreeMap::new(),
    }
}

pub async fn scenario(name: &str, stores: &Stores) {
    let config = stores.create_repo("conf", "desc").await.unwrap();
    assert!(
        stores
            .create_repo("conf", "")
            .await
            .unwrap_err()
            .is_conflict(),
        "{name}"
    );
    assert_eq!(
        stores.get_repo_by_name("conf").await.unwrap(),
        Some(config.clone())
    );
    let repo = stores.repo(config.id);

    let a_cv = cv("a", vec![]);
    let a = repo.put_change_version(&a_cv).await.unwrap();
    assert_eq!(
        repo.put_change_version(&a_cv).await.unwrap(),
        a,
        "{name}: idempotent"
    );
    let b_hash = repo.put_change_version(&cv("b", vec![a])).await.unwrap();
    assert_eq!(
        repo.get_change_version(&a).await.unwrap(),
        Some(a_cv.clone())
    );
    assert_eq!(
        repo.change_versions(&a_cv.change_id).await.unwrap(),
        vec![a]
    );
    let log = repo.log_after(None, 10).await.unwrap();
    assert_eq!(
        log.iter().map(|(_, h)| *h).collect::<Vec<_>>(),
        vec![a, b_hash],
        "{name}"
    );
    assert!(log[0].0 < log[1].0, "{name}: stamps follow commit order");
    assert_eq!(repo.log_head().await.unwrap(), Some(log[1].0), "{name}");
    assert_eq!(
        repo.log_after(Some(log[0].0), 10).await.unwrap(),
        vec![log[1]],
        "{name}"
    );

    assert_eq!(repo.trunk_head().await.unwrap(), None);
    assert_eq!(repo.land_on_trunk(None, a).await.unwrap(), TrunkSeq(1));
    assert!(
        repo.land_on_trunk(None, b_hash)
            .await
            .unwrap_err()
            .is_conflict(),
        "{name}: stale expectation"
    );
    assert_eq!(
        repo.land_on_trunk(Some(a), b_hash).await.unwrap(),
        TrunkSeq(2)
    );
    assert_eq!(
        repo.trunk_head().await.unwrap(),
        Some((TrunkSeq(2), b_hash))
    );
    assert_eq!(repo.trunk_at(TrunkSeq(1)).await.unwrap(), Some(a));
    assert_eq!(repo.trunk_seq_of(&b_hash).await.unwrap(), Some(TrunkSeq(2)));
    assert_eq!(
        repo.trunk_range(TrunkSeq(0), TrunkSeq(2)).await.unwrap(),
        vec![(TrunkSeq(1), a), (TrunkSeq(2), b_hash)]
    );
    let info = repo.get_change_info(&b_hash).await.unwrap().unwrap();
    assert_eq!(info.trunk_seq, Some(TrunkSeq(2)), "{name}");

    // Ref CAS: create, wrong expectation, update, delete.
    let feature = rill_types::RefName::new("feature").unwrap();
    repo.update_ref(&feature, None, Some(a)).await.unwrap();
    assert!(repo
        .update_ref(&feature, None, Some(b_hash))
        .await
        .unwrap_err()
        .is_conflict());
    repo.update_ref(&feature, Some(a), Some(b_hash))
        .await
        .unwrap();
    assert_eq!(repo.get_ref(&feature).await.unwrap(), Some(b_hash));
    let refs = repo.list_refs().await.unwrap();
    assert_eq!(refs.len(), 2, "{name}: trunk + feature");
    repo.update_ref(&feature, Some(b_hash), None).await.unwrap();
    assert_eq!(repo.get_ref(&feature).await.unwrap(), None);

    repo.add_relation("doc", "owner", "alice").await.unwrap();
    repo.add_relation("doc", "viewer", "bob").await.unwrap();
    assert_eq!(
        repo.relations_of("doc").await.unwrap(),
        vec![
            ("owner".to_string(), "alice".to_string()),
            ("viewer".to_string(), "bob".to_string())
        ]
    );
    repo.add_relation("doc/x", "owner", "carol").await.unwrap();
    repo.remove_relation("doc", "owner", "alice").await.unwrap();
    assert_eq!(repo.relations_of("doc").await.unwrap().len(), 1, "{name}");
    assert_eq!(repo.list_relations().await.unwrap().len(), 2, "{name}");
}
