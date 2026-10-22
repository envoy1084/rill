//! A release lander: lands on its release under its own lease and with
//! the rules on the release, never on trunk.

use std::sync::Arc;

use rill_lander::{lander_lease, AlwaysGreen, LandStatus, NoopHooks};
use rill_types::{Line, Principal, RiskClass, TrunkSeq};

use crate::common::{fast_policy, request, Fixture};

#[tokio::test]
async fn a_release_lands_cherry_picks_under_its_own_rules_and_lease() {
    let fx = Fixture::new().await;
    // The release is cut where the registry merges by union; trunk then
    // drops the driver.
    let (cut, _) = fx
        .commit(
            None,
            &[
                (
                    ".rill/merge.toml",
                    "[[driver]]\npaths = [\"registry\"]\nmerge = \"union\"\n",
                ),
                ("registry", "a\n"),
            ],
            "cut",
        )
        .await;
    fx.repo.land_on_trunk(None, cut).await.unwrap();
    fx.repo
        .cut_release("1.0", cut, &Principal::human("rel"))
        .await
        .unwrap();
    let (later, _) = fx
        .delete(cut, ".rill/merge.toml", "drop the driver on trunk")
        .await;
    fx.repo.land_on_trunk(Some(cut), later).await.unwrap();

    let line = Line::Release("1.0".into());
    let release = Arc::new(
        fx.bare_lander(Arc::new(NoopHooks), Arc::new(AlwaysGreen), fast_policy())
            .on_line(line.clone()),
    );
    let (_, x) = fx.commit(Some(cut), &[("registry", "a\nx\n")], "x").await;
    let (_, y) = fx.commit(Some(cut), &[("registry", "a\ny\n")], "y").await;
    for (cv, seq) in [(&x, 1), (&y, 2)] {
        assert_eq!(
            release
                .land_now(request(cv, RiskClass::Normal).onto(line.clone()))
                .await
                .unwrap(),
            LandStatus::Landed { seq: TrunkSeq(seq) },
            "the release's own driver resolves the second"
        );
    }
    let (_, head) = fx.repo.line_head(&line).await.unwrap().unwrap();
    assert_eq!(
        fx.file(fx.root(&head).await, "registry").await.as_deref(),
        Some("a\nx\ny\n")
    );
    assert_eq!(fx.head().await, (TrunkSeq(2), later), "trunk did not move");
    assert!(release.held_lease().await.is_some());
    assert_eq!(lander_lease(&line), "lander:release/1.0");

    assert!(
        release
            .enqueue(request(&x, RiskClass::Normal))
            .await
            .is_err(),
        "a trunk request is not the release lander's"
    );
}
