//! Skip-pointer ancestry agrees with walking parents one by one.

mod common;

use std::collections::{HashMap, HashSet};

use rill_core::ChangeBuilder;
use rill_types::ChangeVersionHash;

use common::{fixture, t};

/// A small deterministic generator, so failures replay.
struct Rng(u64);

impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

fn ancestors_of(
    parents: &HashMap<ChangeVersionHash, Option<ChangeVersionHash>>,
    v: ChangeVersionHash,
) -> Vec<ChangeVersionHash> {
    let mut out = vec![v];
    let mut cur = parents[&v];
    while let Some(p) = cur {
        out.push(p);
        cur = parents[&p];
    }
    out
}

#[tokio::test]
async fn fast_ancestry_matches_brute_force_on_branching_histories() {
    let f = fixture().await;
    let mut rng = Rng(42);
    let mut versions: Vec<ChangeVersionHash> = Vec::new();
    let mut parents = HashMap::new();
    // Two roots, then mostly linear growth with frequent branches, like
    // trunk plus many changes based on older trunk versions.
    for i in 0..400 {
        let parent = match i {
            0 | 1 => None,
            _ if rng.below(3) == 0 => Some(versions[rng.below(versions.len())]),
            _ => versions.last().copied(),
        };
        let mut b = ChangeBuilder::new(f.alice.clone())
            .message(format!("v{i}"))
            .created_at(t(i as i64));
        if let Some(p) = parent {
            b = b.parent(p);
        }
        let (hash, _) = b
            .file(&format!("f{i}"), "x")
            .unwrap()
            .commit(&f.repo)
            .await
            .unwrap();
        parents.insert(hash, parent);
        versions.push(hash);
    }
    for _ in 0..2_000 {
        let a = versions[rng.below(versions.len())];
        let b = versions[rng.below(versions.len())];
        let chain_a = ancestors_of(&parents, a);
        let chain_b: HashSet<_> = ancestors_of(&parents, b).into_iter().collect();
        assert_eq!(
            f.graph.is_ancestor(&a, &b).await.unwrap(),
            chain_b.contains(&a),
            "is_ancestor({a}, {b})"
        );
        let expected = chain_a.into_iter().find(|v| chain_b.contains(v));
        assert_eq!(
            f.graph.merge_base(&a, &b).await.unwrap(),
            expected,
            "merge_base({a}, {b})"
        );
    }
}

#[tokio::test]
async fn deep_linear_history_needs_few_steps() {
    let f = fixture().await;
    let mut tip = None;
    let mut first = None;
    for i in 0..3_000 {
        let mut b = ChangeBuilder::new(f.alice.clone()).created_at(t(i));
        if let Some(p) = tip {
            b = b.parent(p);
        }
        let (hash, _) = b
            .file("counter", i.to_string())
            .unwrap()
            .commit(&f.repo)
            .await
            .unwrap();
        first.get_or_insert(hash);
        tip = Some(hash);
    }
    let (tip, first) = (tip.unwrap(), first.unwrap());
    let info = f.graph.ancestry(&tip).await.unwrap();
    assert_eq!(info.depth, 2_999);
    assert!(info.jump_depth < info.depth);
    assert_eq!(
        f.graph.ancestor_at_depth(&tip, 0).await.unwrap(),
        Some(first)
    );
    assert!(f.graph.is_ancestor(&first, &tip).await.unwrap());
    assert!(!f.graph.is_ancestor(&tip, &first).await.unwrap());
}
