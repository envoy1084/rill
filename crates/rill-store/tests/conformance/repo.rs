//! Typed repository access on every backend.

use std::collections::BTreeMap;

use rill_store::repo::{LandBatch, LandEntry};
use rill_store::Stores;
use rill_types::{
    ChangeId, ChangeVersion, ChangeVersionHash, Principal, Provenance, ReviewState, ReviewStatus,
    RiskClass, Timestamp, TrunkSeq,
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
    repo.update_ref(&feature, None, Some(a), &Principal::human("alice"))
        .await
        .unwrap();
    assert!(repo
        .update_ref(&feature, None, Some(b_hash), &Principal::human("alice"))
        .await
        .unwrap_err()
        .is_conflict());
    repo.update_ref(&feature, Some(a), Some(b_hash), &Principal::human("alice"))
        .await
        .unwrap();
    assert_eq!(repo.get_ref(&feature).await.unwrap(), Some(b_hash));
    let refs = repo.list_refs().await.unwrap();
    assert_eq!(refs.len(), 2, "{name}: trunk + feature");
    repo.update_ref(&feature, Some(b_hash), None, &Principal::human("alice"))
        .await
        .unwrap();
    assert_eq!(repo.get_ref(&feature).await.unwrap(), None);

    repo.add_relation("doc", "owner", "alice", &Principal::human("alice"))
        .await
        .unwrap();
    repo.add_relation("doc", "viewer", "bob", &Principal::human("alice"))
        .await
        .unwrap();
    assert_eq!(
        repo.relations_of("doc").await.unwrap(),
        vec![
            ("owner".to_string(), "alice".to_string()),
            ("viewer".to_string(), "bob".to_string())
        ]
    );
    repo.add_relation("doc/x", "owner", "carol", &Principal::human("alice"))
        .await
        .unwrap();
    repo.remove_relation("doc", "owner", "alice", &Principal::human("alice"))
        .await
        .unwrap();
    assert_eq!(repo.relations_of("doc").await.unwrap().len(), 1, "{name}");
    assert_eq!(repo.list_relations().await.unwrap().len(), 2, "{name}");
}

fn review_of(change: ChangeId, version: ChangeVersionHash, status: ReviewStatus) -> ReviewState {
    ReviewState {
        change_id: change,
        versions: vec![version],
        status,
        intent: None,
        owner: Principal::human("alice"),
        votes: Vec::new(),
        threads: Vec::new(),
        risk: RiskClass::Normal,
        risk_reasons: Vec::new(),
        stack: Vec::new(),
        landed_seq: None,
        created_at: Timestamp::from_millis(1),
        updated_at: Timestamp::from_millis(1),
        extra: BTreeMap::new(),
    }
}

/// The land transaction: batches, records, reviews and fencing.
pub async fn landing(name: &str, stores: &Stores) {
    let repo = stores.repo(stores.create_repo("landing", "").await.unwrap().id);
    let root_cv = cv("root", vec![]);
    let root = repo.put_change_version(&root_cv).await.unwrap();
    assert_eq!(repo.land_on_trunk(None, root).await.unwrap(), TrunkSeq(1));

    let mut entries = Vec::new();
    for i in 0..3 {
        let version = cv(&format!("v{i}"), vec![root]);
        let hash = repo.put_change_version(&version).await.unwrap();
        repo.create_review(
            &review_of(version.change_id, hash, ReviewStatus::Landing),
            &Principal::human("alice"),
        )
        .await
        .unwrap();
        entries.push(LandEntry {
            version: hash,
            change_id: version.change_id,
            requested_version: hash,
            requester: Principal::agent("bot"),
            risk: RiskClass::Normal,
            review: repo.read_review(&version.change_id).await.unwrap(),
        });
    }
    let lease = repo.try_acquire("lander", "a").await.unwrap().unwrap();
    let batch = LandBatch {
        expected: Some(root),
        entries: entries.clone(),
        fence: Some(lease.clone()),
        actor: None,
        also: rill_store::Transaction::new().set(repo.keys().misc("followup"), "jobs"),
    };
    let landed = repo.land(batch.clone()).await.unwrap();
    assert!(
        repo.meta()
            .get(&repo.keys().misc("followup"))
            .await
            .unwrap()
            .is_some(),
        "{name}: the caller's writes commit with the landing"
    );
    let seqs: Vec<_> = landed.iter().map(|l| l.seq).collect();
    assert_eq!(seqs, vec![TrunkSeq(2), TrunkSeq(3), TrunkSeq(4)], "{name}");
    assert_eq!(
        repo.trunk_head().await.unwrap(),
        Some((TrunkSeq(4), entries[2].version))
    );
    assert_eq!(repo.list_landings(10).await.unwrap().len(), 4, "{name}");
    for e in &entries {
        let review = repo.get_review(&e.change_id).await.unwrap().unwrap();
        assert_eq!(review.status, ReviewStatus::Landed, "{name}");
        assert_eq!(
            repo.landing_of(&e.version)
                .await
                .unwrap()
                .unwrap()
                .change_id,
            e.change_id
        );
    }
    // Landing the same batch again finds trunk moved: nothing lands twice.
    assert!(repo.land(batch).await.unwrap_err().is_conflict(), "{name}");

    // A replaced lander is fenced off trunk.
    let next = cv("next", vec![entries[2].version]);
    let next_hash = repo.put_change_version(&next).await.unwrap();
    repo.release(&lease).await.unwrap();
    repo.try_acquire("lander", "b").await.unwrap().unwrap();
    let stale = LandBatch {
        expected: Some(entries[2].version),
        entries: vec![LandEntry {
            version: next_hash,
            change_id: next.change_id,
            requested_version: next_hash,
            requester: Principal::agent("bot"),
            risk: RiskClass::Normal,
            review: None,
        }],
        fence: Some(lease),
        ..LandBatch::default()
    };
    assert!(
        repo.land(stale).await.unwrap_err().is_conflict(),
        "{name}: fenced"
    );

    // A change abandoned meanwhile does not land.
    repo.create_review(
        &review_of(next.change_id, next_hash, ReviewStatus::Abandoned),
        &Principal::human("alice"),
    )
    .await
    .unwrap();
    let abandoned = LandBatch {
        expected: Some(entries[2].version),
        entries: vec![LandEntry {
            version: next_hash,
            change_id: next.change_id,
            requested_version: next_hash,
            requester: Principal::agent("bot"),
            risk: RiskClass::Normal,
            review: repo.read_review(&next.change_id).await.unwrap(),
        }],
        ..LandBatch::default()
    };
    assert!(repo.land(abandoned).await.is_err(), "{name}");
    assert_eq!(
        repo.trunk_head().await.unwrap().unwrap().0,
        TrunkSeq(4),
        "{name}"
    );
}
