//! Release lines on every backend: cut once, moved only by landings on
//! their own head, never touching trunk.

use rill_store::repo::{LandBatch, LandEntry};
use rill_store::Stores;
use rill_types::{
    ChangeVersionHash, LandRequest, Line, Principal, RefName, ReviewStatus, RiskClass, TrunkSeq,
};

use crate::repo::{cv, review_of};

fn entry(version: ChangeVersionHash, change_id: rill_types::ChangeId) -> LandEntry {
    LandEntry {
        version,
        change_id,
        requested_version: version,
        requester: Principal::agent("bot"),
        risk: RiskClass::Normal,
        review: None,
        policy: None,
        approvals: Vec::new(),
    }
}

pub async fn scenario(name: &str, stores: &Stores) {
    let repo = stores.repo(stores.create_repo("releases", "").await.unwrap().id);
    let alice = Principal::human("alice");
    let root = repo.put_change_version(&cv("root", vec![])).await.unwrap();
    repo.land_on_trunk(None, root).await.unwrap();
    let line = Line::Release("1.0".into());

    let fix_cv = cv("fix", vec![root]);
    let fix = repo.put_change_version(&fix_cv).await.unwrap();
    repo.create_review(
        &review_of(fix_cv.change_id, fix, ReviewStatus::Landing),
        &alice,
    )
    .await
    .unwrap();
    let mut batch = LandBatch {
        line: line.clone(),
        expected: Some(root),
        entries: vec![entry(fix, fix_cv.change_id)],
        ..LandBatch::default()
    };
    assert!(
        repo.land(batch.clone()).await.unwrap_err().is_not_found(),
        "{name}: nothing lands on a release before it is cut"
    );

    repo.cut_release("1.0", root, &alice).await.unwrap();
    assert!(
        repo.cut_release("1.0", fix, &alice)
            .await
            .unwrap_err()
            .is_conflict(),
        "{name}: a release is cut once"
    );
    assert_eq!(
        repo.line_head(&line).await.unwrap(),
        Some((TrunkSeq(0), root))
    );

    batch.entries[0].review = repo.read_review(&fix_cv.change_id).await.unwrap();
    let landed = repo.land(batch.clone()).await.unwrap();
    assert_eq!(landed[0].seq, TrunkSeq(1), "{name}");
    assert_eq!(
        repo.line_head(&line).await.unwrap(),
        Some((TrunkSeq(1), fix))
    );
    assert_eq!(
        repo.get_ref(&RefName::release("1.0").unwrap())
            .await
            .unwrap(),
        Some(fix)
    );
    assert_eq!(
        repo.line_seq_of(&line, &fix).await.unwrap(),
        Some(TrunkSeq(1))
    );
    assert_eq!(
        repo.landing_on(&line, TrunkSeq(1))
            .await
            .unwrap()
            .unwrap()
            .change_id,
        fix_cv.change_id
    );
    assert_eq!(
        repo.trunk_head().await.unwrap(),
        Some((TrunkSeq(1), root)),
        "{name}: trunk did not move"
    );
    assert_eq!(repo.trunk_seq_of(&fix).await.unwrap(), None);
    let review = repo.get_review(&fix_cv.change_id).await.unwrap().unwrap();
    assert_eq!(
        (review.status, review.landed_seq),
        (ReviewStatus::Landed, None),
        "{name}: a trunk position is only for trunk"
    );
    assert!(
        repo.land(batch).await.unwrap_err().is_conflict(),
        "{name}: the release moved"
    );

    let request = LandRequest::new(
        fix_cv.change_id,
        fix,
        Principal::agent("bot"),
        RiskClass::Normal,
    )
    .onto(line.clone());
    repo.queue_land(&request).await.unwrap();
    assert!(
        repo.pending_land_requests(10).await.unwrap().is_empty(),
        "{name}"
    );
    assert_eq!(
        repo.pending_land_requests_on(&line, 10)
            .await
            .unwrap()
            .len(),
        1,
        "{name}: a release request waits in the release's queue"
    );
}
