//! Hashing content hashes for in-memory maps: they are BLAKE3 digests
//! already, so their first eight bytes are as good a hash as any, and
//! SipHash over all 32 was a large share of a query's time.

use std::hash::{BuildHasherDefault, Hasher};

/// Takes the leading eight bytes of the last key bytes written.
#[derive(Default)]
pub struct ContentHasher(u64);

impl Hasher for ContentHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        if let Some(head) = bytes.first_chunk::<8>() {
            self.0 = u64::from_le_bytes(*head);
        } else {
            for &b in bytes {
                self.0 = self.0.rotate_left(8) ^ u64::from(b);
            }
        }
    }
}

/// A map keyed by content hash.
pub type ContentMap<V> =
    std::collections::HashMap<rill_types::ObjectHash, V, BuildHasherDefault<ContentHasher>>;

#[cfg(test)]
mod tests {
    use super::*;
    use rill_types::{ObjectHash, ObjectKind};

    #[test]
    fn distinct_contents_hash_apart() {
        let mut m: ContentMap<u32> = ContentMap::default();
        for i in 0..10_000u32 {
            m.insert(ObjectHash::digest(ObjectKind::Blob, &i.to_le_bytes()), i);
        }
        assert_eq!(m.len(), 10_000);
        let h = ObjectHash::digest(ObjectKind::Blob, &7u32.to_le_bytes());
        assert_eq!(m.get(&h), Some(&7));
    }
}
