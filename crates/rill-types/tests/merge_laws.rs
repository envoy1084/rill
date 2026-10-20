//! Algebraic laws of `Merge<T>`, checked against its meaning as a signed
//! multiset: `add0 - remove0 + add1 - ...`.

use std::collections::BTreeMap;

use proptest::prelude::*;
use rill_types::Merge;

fn value(m: &Merge<u8>) -> BTreeMap<u8, i32> {
    let mut out = BTreeMap::new();
    for a in m.adds() {
        *out.entry(*a).or_insert(0) += 1;
    }
    for r in m.removes() {
        *out.entry(*r).or_insert(0) -= 1;
    }
    out.retain(|_, n| *n != 0);
    out
}

/// Small odd-length term lists over a tiny domain, so terms collide often.
fn merge() -> impl Strategy<Value = Merge<u8>> {
    (0usize..5)
        .prop_flat_map(|sides| prop::collection::vec(0u8..4, 2 * sides + 1))
        .prop_map(|v| Merge::from_vec(v).unwrap())
}

proptest! {
    #[test]
    fn simplify_preserves_value(m in merge()) {
        prop_assert_eq!(value(&m.clone().simplify()), value(&m));
    }

    #[test]
    fn simplify_leaves_no_cancellable_pair(m in merge()) {
        let s = m.simplify();
        let removes: Vec<_> = s.removes().collect();
        prop_assert!(s.adds().all(|a| !removes.contains(&a)));
    }

    #[test]
    fn flatten_matches_nested_value(outer in prop::collection::vec(merge(), 1..4)) {
        let len = if outer.len() % 2 == 0 { outer.len() - 1 } else { outer.len() };
        let outer: Vec<Merge<u8>> = outer.into_iter().take(len).collect();
        let mut expected: BTreeMap<u8, i32> = BTreeMap::new();
        for (i, inner) in outer.iter().enumerate() {
            let sign = if i % 2 == 0 { 1 } else { -1 };
            for (k, n) in value(inner) {
                *expected.entry(k).or_insert(0) += sign * n;
            }
        }
        expected.retain(|_, n| *n != 0);
        let nested = Merge::from_vec(outer).unwrap();
        prop_assert_eq!(value(&nested.flatten()), expected);
    }
}
