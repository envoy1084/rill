//! The commit graph's cached version metadata.

use rill_core::ChangeBuilder;
use rill_store::repo::{LandBatch, LandEntry};
use rill_types::{RiskClass, TrunkSeq};

use crate::common::{fixture, t};

/// Version metadata is cached once it is on trunk; a version read before
/// it lands must not keep reporting that it is not on trunk.
#[tokio::test]
async fn a_version_read_before_landing_reports_its_trunk_position_after() {
    let f = fixture().await;
    let (version, cv) = ChangeBuilder::new(f.alice.clone())
        .message("one")
        .created_at(t(1))
        .file("a.txt", "a\n")
        .unwrap()
        .commit(&f.repo)
        .await
        .unwrap();
    assert_eq!(f.graph.info(&version).await.unwrap().trunk_seq, None);

    f.repo
        .land(LandBatch {
            line: Default::default(),
            expected: None,
            entries: vec![LandEntry {
                version,
                change_id: cv.change_id,
                requested_version: version,
                requester: f.alice.clone(),
                risk: RiskClass::Normal,
                review: None,
                policy: None,
                approvals: Vec::new(),
            }],
            fence: None,
            actor: None,
            also: Default::default(),
        })
        .await
        .unwrap();
    for _ in 0..2 {
        let info = f.graph.info(&version).await.unwrap();
        assert_eq!(info.trunk_seq, Some(TrunkSeq(1)));
    }
}
