//! A Bloom filter over byte strings.

use serde::{Deserialize, Serialize};

/// Bits per inserted item; with [`HASHES`] gives about 1% false positives.
const BITS_PER_ITEM: usize = 10;
const HASHES: u64 = 7;

/// A set that answers "maybe present" or "certainly absent".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Bloom {
    bits: Vec<u64>,
}

impl Bloom {
    /// A filter sized for `items` insertions.
    pub fn with_capacity(items: usize) -> Self {
        let words = (items * BITS_PER_ITEM).div_ceil(64).max(1);
        Bloom {
            bits: vec![0; words],
        }
    }

    fn positions(&self, item: &[u8]) -> impl Iterator<Item = usize> + '_ {
        let digest = blake3::hash(item);
        let bytes = digest.as_bytes();
        let a = u64::from_le_bytes(bytes[..8].try_into().expect("32-byte digest"));
        let b = u64::from_le_bytes(bytes[8..16].try_into().expect("32-byte digest")) | 1;
        let m = (self.bits.len() * 64) as u64;
        (0..HASHES).map(move |i| (a.wrapping_add(i.wrapping_mul(b)) % m) as usize)
    }

    pub fn insert(&mut self, item: &[u8]) {
        let positions: Vec<usize> = self.positions(item).collect();
        for p in positions {
            self.bits[p / 64] |= 1 << (p % 64);
        }
    }

    pub fn may_contain(&self, item: &[u8]) -> bool {
        self.positions(item)
            .all(|p| self.bits[p / 64] & (1 << (p % 64)) != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_no_false_negatives_and_few_false_positives() {
        let mut bloom = Bloom::with_capacity(10_000);
        for i in 0..10_000 {
            bloom.insert(format!("in-{i}").as_bytes());
        }
        assert!((0..10_000).all(|i| bloom.may_contain(format!("in-{i}").as_bytes())));
        let false_positives = (0..10_000)
            .filter(|i| bloom.may_contain(format!("out-{i}").as_bytes()))
            .count();
        assert!(false_positives < 300, "{false_positives} false positives");
    }
}
