//! A small bounded cache with approximate least-recently-used eviction.
//!
//! Entries carry a monotonically increasing "last touched" stamp; when the
//! cache exceeds its capacity the oldest eighth is evicted in one pass. That
//! keeps eviction amortized O(1) per insert without a linked list, which is
//! all a cache of decoded objects needs.

use std::collections::HashMap;
use std::hash::Hash;

use parking_lot::Mutex;

struct Inner<K, V> {
    map: HashMap<K, (V, u64)>,
    clock: u64,
    capacity: usize,
}

/// Thread-safe bounded cache; values are cloned out, so wrap large ones in
/// an `Arc`.
pub struct LruCache<K, V> {
    inner: Mutex<Inner<K, V>>,
}

impl<K: Hash + Eq + Clone, V: Clone> LruCache<K, V> {
    /// Create a cache holding at most `capacity` entries (minimum 8).
    pub fn new(capacity: usize) -> Self {
        LruCache {
            inner: Mutex::new(Inner {
                map: HashMap::new(),
                clock: 0,
                capacity: capacity.max(8),
            }),
        }
    }

    /// Fetch a value, marking it recently used.
    pub fn get(&self, key: &K) -> Option<V> {
        let mut inner = self.inner.lock();
        inner.clock += 1;
        let clock = inner.clock;
        let entry = inner.map.get_mut(key)?;
        entry.1 = clock;
        Some(entry.0.clone())
    }

    /// Insert or replace a value.
    pub fn put(&self, key: K, value: V) {
        let mut inner = self.inner.lock();
        inner.clock += 1;
        let clock = inner.clock;
        inner.map.insert(key, (value, clock));
        if inner.map.len() > inner.capacity {
            let evict = inner.capacity / 8 + 1;
            let mut stamps: Vec<(u64, K)> = inner
                .map
                .iter()
                .map(|(k, (_, s))| (*s, k.clone()))
                .collect();
            stamps.sort_unstable_by_key(|(s, _)| *s);
            for (_, k) in stamps.into_iter().take(evict) {
                inner.map.remove(&k);
            }
        }
    }

    /// Number of cached entries.
    pub fn len(&self) -> usize {
        self.inner.lock().map.len()
    }

    /// True when nothing is cached.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_least_recently_used() {
        let cache = LruCache::new(8);
        for i in 0..8u32 {
            cache.put(i, i);
        }
        // Touch 0 so it survives the eviction triggered by the ninth insert.
        assert_eq!(cache.get(&0), Some(0));
        cache.put(8, 8);
        assert!(cache.len() <= 8);
        assert_eq!(cache.get(&0), Some(0));
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&8), Some(8));
    }
}
