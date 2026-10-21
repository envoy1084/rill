//! The background deriver keeps trunk's derived data current.

mod common;

use rill_core::derive::{BackgroundDeriver, ChangeSummary, PathHistory, MAX_ATTEMPTS};
use rill_core::ChangeBuilder;
use rill_types::{ObjectHash, ObjectKind, TrunkSeq};

use common::{fixture, t};

#[tokio::test]
async fn derives_trunk_in_order_resumes_and_skips_what_keeps_failing() {
    let f = fixture().await;
    let mut tip = None;
    let mut landed = Vec::new();
    for i in 0..5 {
        let mut b = ChangeBuilder::new(f.alice.clone()).created_at(t(i));
        if let Some(p) = tip {
            b = b.parent(p);
        }
        let (hash, _) = b
            .file("f", i.to_string())
            .unwrap()
            .commit(&f.repo)
            .await
            .unwrap();
        f.repo.land_on_trunk(tip, hash).await.unwrap();
        landed.push(hash);
        tip = Some(hash);
    }

    let deriver = BackgroundDeriver::new(f.derive.clone());
    let pass = deriver.pass(3).await.unwrap();
    assert_eq!((pass.versions, pass.cursor), (3, TrunkSeq(3)));
    assert!(f.derive.has::<PathHistory>(&landed[2]).await.unwrap());
    assert!(!f.derive.has::<ChangeSummary>(&landed[3]).await.unwrap());

    // A new deriver (a restart) continues after the stored cursor.
    let restarted = BackgroundDeriver::new(f.derive.clone());
    assert_eq!(restarted.pass(10).await.unwrap().cursor, TrunkSeq(5));
    assert!(f.derive.has::<ChangeSummary>(&landed[4]).await.unwrap());

    // A trunk entry that cannot be derived is retried, then skipped.
    let missing = ObjectHash::digest(ObjectKind::ChangeVersion, b"never stored");
    f.repo.land_on_trunk(tip, missing).await.unwrap();
    for _ in 1..MAX_ATTEMPTS {
        assert_eq!(restarted.pass(10).await.unwrap().versions, 0);
    }
    let pass = restarted.pass(10).await.unwrap();
    assert_eq!((pass.abandoned, pass.cursor), (1, TrunkSeq(6)));
}
