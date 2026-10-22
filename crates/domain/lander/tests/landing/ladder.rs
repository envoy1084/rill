//! The conflict ladder: merge drivers resolve what the rebase could not,
//! and what they cannot goes to a resolver agent and the owner.

use std::sync::Arc;

use rill_core::TreeOps;
use rill_lander::{AlwaysGreen, LandStatus, MERGE_DRIVERS_EXTRA};
use rill_types::{ChangeVersionHash, RepoPathBuf, RiskClass, TrunkSeq};
use tokio::sync::Mutex;

use crate::common::{fast_policy, request, Fixture};
use crate::doubles::{ListingRegenerator, RecordingHooks, TakingResolver};

const DRIVERS: &str = r#"
[[driver]]
paths = ["registry.txt"]
merge = "union"

[[driver]]
name = "deps"
paths = ["deps.lock"]
merge = "regenerate"
command = ["list-deps"]

[ladder]
manual = ["auth/**"]
"#;

/// Trunk with the drivers, `.gitattributes` and `files`, landed.
async fn trunk(fx: &Fixture, files: &[(&str, &str)]) -> ChangeVersionHash {
    let mut all = vec![
        (".rill/merge.toml", DRIVERS),
        (".gitattributes", "CHANGELOG.md merge=union\n"),
    ];
    all.extend_from_slice(files);
    let (base, _) = fx.commit(None, &all, "base").await;
    fx.repo.land_on_trunk(None, base).await.unwrap();
    base
}

fn regenerator(fx: &Fixture, fail: Option<&str>) -> Arc<ListingRegenerator> {
    Arc::new(ListingRegenerator {
        trees: TreeOps::clone(&fx.trees),
        fail: fail.map(str::to_string),
        seen: Mutex::new(Vec::new()),
    })
}

#[tokio::test]
async fn union_drivers_keep_both_sides_entries() {
    let fx = Fixture::new().await;
    let lander = fx.lander(
        Arc::new(RecordingHooks::default()),
        Arc::new(AlwaysGreen),
        fast_policy(),
    );
    let base = trunk(&fx, &[("registry.txt", "a\n"), ("CHANGELOG.md", "# 1\n")]).await;
    let (_, x) = fx
        .commit(
            Some(base),
            &[("registry.txt", "a\nx\n"), ("CHANGELOG.md", "# 1\n- x\n")],
            "x",
        )
        .await;
    let (_, y) = fx
        .commit(
            Some(base),
            &[("registry.txt", "a\ny\n"), ("CHANGELOG.md", "# 1\n- y\n")],
            "y",
        )
        .await;
    for (cv, seq) in [(&y, 2), (&x, 3)] {
        assert_eq!(
            lander
                .land_now(request(cv, RiskClass::Normal))
                .await
                .unwrap(),
            LandStatus::Landed { seq: TrunkSeq(seq) }
        );
    }
    let (_, head) = fx.head().await;
    let root = fx.root(&head).await;
    assert_eq!(
        fx.file(root, "registry.txt").await.as_deref(),
        Some("a\ny\nx\n")
    );
    assert_eq!(
        fx.file(root, "CHANGELOG.md").await.as_deref(),
        Some("# 1\n- y\n- x\n"),
        ".gitattributes merge=union applies where no driver is declared"
    );
    let landed = fx.repo.get_change_version(&head).await.unwrap().unwrap();
    assert_eq!(
        landed.extra.get(MERGE_DRIVERS_EXTRA).map(|b| b.as_ref()),
        Some(&b"gitattributes union: CHANGELOG.md; union: registry.txt"[..])
    );
}

#[tokio::test]
async fn regenerate_drivers_rebuild_from_the_merged_tree() {
    let fx = Fixture::new().await;
    let regen = regenerator(&fx, None);
    let lander = Arc::new(
        fx.bare_lander(
            Arc::new(RecordingHooks::default()),
            Arc::new(AlwaysGreen),
            fast_policy(),
        )
        .with_regenerator(regen.clone()),
    );
    let base = trunk(&fx, &[("deps/a", "1"), ("deps.lock", "a\n")]).await;
    let (_, x) = fx
        .commit(Some(base), &[("deps/x", "1"), ("deps.lock", "a\nx\n")], "x")
        .await;
    let (_, y) = fx
        .commit(Some(base), &[("deps/y", "1"), ("deps.lock", "a\ny\n")], "y")
        .await;
    lander
        .land_now(request(&y, RiskClass::Normal))
        .await
        .unwrap();
    assert_eq!(
        lander
            .land_now(request(&x, RiskClass::Normal))
            .await
            .unwrap(),
        LandStatus::Landed { seq: TrunkSeq(3) }
    );
    let (_, head) = fx.head().await;
    assert_eq!(
        fx.file(fx.root(&head).await, "deps.lock").await.as_deref(),
        Some("a\nx\ny\n")
    );
    let seen = regen.seen.lock().await;
    assert_eq!(seen.len(), 1, "only the conflicted landing regenerates");
    assert_eq!(
        seen[0].outputs,
        vec![RepoPathBuf::new("deps.lock").unwrap()]
    );
    assert_eq!(seen[0].command, vec!["list-deps".to_string()]);
    assert_eq!(
        fx.file(seen[0].root, "deps.lock").await.as_deref(),
        Some("a\ny\n"),
        "the command starts from trunk's version"
    );
}

#[tokio::test]
async fn what_no_driver_resolves_goes_to_the_resolver_unless_manual() {
    let fx = Fixture::new().await;
    let hooks = Arc::new(RecordingHooks::default());
    let resolver = Arc::new(TakingResolver::default());
    let lander = Arc::new(
        fx.bare_lander(hooks.clone(), Arc::new(AlwaysGreen), fast_policy())
            .with_resolver(resolver.clone()),
    );
    let base = trunk(&fx, &[("src/a.rs", "a\n"), ("auth/k.rs", "k\n")]).await;
    for path in ["src/a.rs", "auth/k.rs"] {
        let (_, x) = fx.commit(Some(base), &[(path, "x\n")], "x").await;
        let (_, y) = fx.commit(Some(base), &[(path, "y\n")], "y").await;
        lander
            .land_now(request(&y, RiskClass::Normal))
            .await
            .unwrap();
        assert_eq!(
            lander
                .land_now(request(&x, RiskClass::Normal))
                .await
                .unwrap(),
            LandStatus::Conflict {
                paths: vec![RepoPathBuf::new(path).unwrap()]
            }
        );
    }
    assert_eq!(
        *resolver.offered.lock().await,
        vec![vec![RepoPathBuf::new("src/a.rs").unwrap()]],
        "a manual path is never offered to an agent"
    );
    let returned = hooks.returned.lock().await;
    assert_eq!(returned.len(), 2, "the owner always hears of it");
    assert!(returned[0].1.ends_with("; agent:fixer is resolving it"));
    assert!(!returned[1].1.contains("agent:fixer"));
}

#[tokio::test]
async fn a_failed_regeneration_returns_the_change_saying_why() {
    let fx = Fixture::new().await;
    let hooks = Arc::new(RecordingHooks::default());
    let lander = Arc::new(
        fx.bare_lander(hooks.clone(), Arc::new(AlwaysGreen), fast_policy())
            .with_regenerator(regenerator(&fx, Some("no registry offline"))),
    );
    let base = trunk(&fx, &[("deps.lock", "a\n")]).await;
    let (_, x) = fx.commit(Some(base), &[("deps.lock", "x\n")], "x").await;
    let (_, y) = fx.commit(Some(base), &[("deps.lock", "y\n")], "y").await;
    lander
        .land_now(request(&y, RiskClass::Normal))
        .await
        .unwrap();
    assert!(matches!(
        lander
            .land_now(request(&x, RiskClass::Normal))
            .await
            .unwrap(),
        LandStatus::Conflict { .. }
    ));
    let returned = hooks.returned.lock().await;
    assert!(
        returned[0]
            .1
            .contains("regenerate driver \"deps\" failed: no registry offline"),
        "{}",
        returned[0].1
    );
}

#[tokio::test]
async fn what_a_driver_resolved_stays_recorded_when_its_landing_is_lined_up() {
    let fx = Fixture::new().await;
    let lander = fx.lander(
        Arc::new(RecordingHooks::default()),
        Arc::new(AlwaysGreen),
        fast_policy(),
    );
    let base = trunk(&fx, &[("registry.txt", "a\n"), ("other", "0\n")]).await;
    let (moved, _) = fx
        .commit(Some(base), &[("registry.txt", "a\nz\n")], "z")
        .await;
    fx.repo.land_on_trunk(Some(base), moved).await.unwrap();
    // A disjoint request lands first, so the resolved one is re-parented
    // onto it when the batch is lined up.
    let (_, first) = fx.commit(Some(moved), &[("other", "1\n")], "other").await;
    let (_, resolved) = fx
        .commit(Some(base), &[("registry.txt", "a\ny\n")], "y")
        .await;
    // Queued a second apart, so the disjoint one is served first.
    let mut earlier = request(&first, RiskClass::Normal);
    earlier.requested_at = rill_types::Timestamp::from_millis(earlier.requested_at.millis() - 1000);
    lander.enqueue(earlier).await.unwrap();
    lander
        .enqueue(request(&resolved, RiskClass::Normal))
        .await
        .unwrap();
    lander.run_once().await.unwrap();
    let (seq, head) = fx.head().await;
    assert_eq!(seq, TrunkSeq(4), "both landed in one batch");
    let landed = fx.repo.get_change_version(&head).await.unwrap().unwrap();
    assert_eq!(landed.change_id, resolved.change_id);
    assert_eq!(
        landed.extra.get(MERGE_DRIVERS_EXTRA).map(|b| b.as_ref()),
        Some(&b"union: registry.txt"[..])
    );
}
