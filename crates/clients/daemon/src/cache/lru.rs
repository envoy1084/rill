//! Least-recently-used accounting with pins and a byte budget. Pure: the
//! caller deletes what [`Lru::insert`] evicts.

use std::collections::{BTreeMap, HashMap};

use rill_types::ObjectHash;

#[derive(Debug, Clone, Copy)]
struct Slot {
    size: u64,
    tick: u64,
    pins: u32,
}

/// Objects by recency. Pinned objects are never evicted and sit outside
/// the recency order until unpinned.
#[derive(Debug, Default)]
pub struct Lru {
    slots: HashMap<ObjectHash, Slot>,
    order: BTreeMap<u64, ObjectHash>,
    tick: u64,
    bytes: u64,
    budget: u64,
}

impl Lru {
    /// An empty cache holding at most `budget` unpinned bytes.
    pub fn new(budget: u64) -> Self {
        Lru {
            budget,
            ..Lru::default()
        }
    }

    /// Bytes held, pinned included.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Objects held.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// True when nothing is held.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The byte budget.
    pub fn budget(&self) -> u64 {
        self.budget
    }

    /// Whether `hash` is held.
    pub fn contains(&self, hash: &ObjectHash) -> bool {
        self.slots.contains_key(hash)
    }

    /// Track an object as most recently used and return what had to be
    /// evicted to stay within the budget.
    pub fn insert(&mut self, hash: ObjectHash, size: u64) -> Vec<ObjectHash> {
        if self.slots.contains_key(&hash) {
            self.touch(&hash);
            return Vec::new();
        }
        self.tick += 1;
        self.slots.insert(
            hash,
            Slot {
                size,
                tick: self.tick,
                pins: 0,
            },
        );
        self.order.insert(self.tick, hash);
        self.bytes += size;
        self.evict()
    }

    /// Mark `hash` most recently used; false when it is not held.
    pub fn touch(&mut self, hash: &ObjectHash) -> bool {
        let Some(slot) = self.slots.get_mut(hash) else {
            return false;
        };
        if slot.pins == 0 {
            self.order.remove(&slot.tick);
            self.tick += 1;
            slot.tick = self.tick;
            self.order.insert(self.tick, *hash);
        }
        true
    }

    /// Protect `hash` from eviction; false when it is not held.
    pub fn pin(&mut self, hash: &ObjectHash) -> bool {
        let Some(slot) = self.slots.get_mut(hash) else {
            return false;
        };
        if slot.pins == 0 {
            self.order.remove(&slot.tick);
        }
        slot.pins += 1;
        true
    }

    /// Undo one [`Lru::pin`] and return what the budget now evicts.
    pub fn unpin(&mut self, hash: &ObjectHash) -> Vec<ObjectHash> {
        let Some(slot) = self.slots.get_mut(hash) else {
            return Vec::new();
        };
        if slot.pins == 0 {
            return Vec::new();
        }
        slot.pins -= 1;
        if slot.pins == 0 {
            self.tick += 1;
            slot.tick = self.tick;
            self.order.insert(self.tick, *hash);
        }
        self.evict()
    }

    /// Forget `hash`.
    pub fn remove(&mut self, hash: &ObjectHash) {
        if let Some(slot) = self.slots.remove(hash) {
            if slot.pins == 0 {
                self.order.remove(&slot.tick);
            }
            self.bytes -= slot.size;
        }
    }

    fn evict(&mut self) -> Vec<ObjectHash> {
        let mut evicted = Vec::new();
        while self.bytes > self.budget {
            let Some((_, hash)) = self.order.pop_first() else {
                break;
            };
            if let Some(slot) = self.slots.remove(&hash) {
                self.bytes -= slot.size;
            }
            evicted.push(hash);
        }
        evicted
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use rill_types::ObjectKind;

    fn h(n: u8) -> ObjectHash {
        ObjectHash::digest(ObjectKind::Blob, &[n])
    }

    #[test]
    fn evicts_least_recently_used_first() {
        let mut lru = Lru::new(30);
        lru.insert(h(1), 10);
        lru.insert(h(2), 10);
        lru.insert(h(3), 10);
        lru.touch(&h(1));
        assert_eq!(lru.insert(h(4), 10), vec![h(2)]);
        assert!(lru.contains(&h(1)));
    }

    #[derive(Debug, Clone)]
    enum Op {
        Insert(u8, u64),
        Touch(u8),
        Pin(u8),
        Unpin(u8),
    }

    fn op() -> impl Strategy<Value = Op> {
        prop_oneof![
            (0u8..16, 1u64..40).prop_map(|(k, s)| Op::Insert(k, s)),
            (0u8..16).prop_map(Op::Touch),
            (0u8..16).prop_map(Op::Pin),
            (0u8..16).prop_map(Op::Unpin),
        ]
    }

    proptest! {
        #[test]
        fn pinned_objects_survive_and_unpinned_bytes_stay_in_budget(ops in prop::collection::vec(op(), 1..200)) {
            let mut lru = Lru::new(100);
            let mut pins: HashMap<ObjectHash, u32> = HashMap::new();
            for op in ops {
                let evicted = match op {
                    Op::Insert(k, s) => lru.insert(h(k), s),
                    Op::Touch(k) => { lru.touch(&h(k)); vec![] }
                    Op::Pin(k) => {
                        if lru.pin(&h(k)) { *pins.entry(h(k)).or_default() += 1; }
                        vec![]
                    }
                    Op::Unpin(k) => {
                        if let Some(n) = pins.get_mut(&h(k)) {
                            *n -= 1;
                            if *n == 0 { pins.remove(&h(k)); }
                        }
                        lru.unpin(&h(k))
                    }
                };
                for e in &evicted {
                    prop_assert!(!pins.contains_key(e), "evicted a pinned object");
                }
                for p in pins.keys() {
                    prop_assert!(lru.contains(p));
                }
                let pinned: u64 = lru.slots.values().filter(|s| s.pins > 0).map(|s| s.size).sum();
                prop_assert!(lru.bytes() - pinned <= lru.budget());
            }
        }
    }
}
