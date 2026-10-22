//! Trigram extraction from raw bytes.

use std::collections::BTreeSet;

/// Pack three consecutive bytes into one `u32` (big-endian, high byte zero).
#[inline]
pub fn pack(a: u8, b: u8, c: u8) -> u32 {
    (u32::from(a) << 16) | (u32::from(b) << 8) | u32::from(c)
}

/// Inverse of [`pack`].
#[inline]
pub fn unpack(t: u32) -> [u8; 3] {
    [(t >> 16) as u8, (t >> 8) as u8, t as u8]
}

/// ASCII case folding of one byte, applied to every trigram of a
/// case-insensitive index. Non-ASCII bytes are left alone: UTF-8 case
/// folding is not byte-local, so a case-insensitive search over non-ASCII
/// letters still works through the regex, only without trigram pruning.
#[inline]
pub fn fold_byte(b: u8) -> u8 {
    b.to_ascii_lowercase()
}

/// The set of trigrams in `bytes`, folded when `case_insensitive`.
///
/// Inputs shorter than three bytes have no trigrams; such blobs are never
/// candidates for a query that requires a literal, which is correct because
/// the literal cannot occur in them.
pub fn trigrams(bytes: &[u8], case_insensitive: bool) -> BTreeSet<u32> {
    let mut out = BTreeSet::new();
    if bytes.len() < 3 {
        return out;
    }
    if case_insensitive {
        for w in bytes.windows(3) {
            out.insert(pack(fold_byte(w[0]), fold_byte(w[1]), fold_byte(w[2])));
        }
    } else {
        for w in bytes.windows(3) {
            out.insert(pack(w[0], w[1], w[2]));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_every_window() {
        let t = trigrams(b"abcd", false);
        assert_eq!(t.len(), 2);
        assert!(t.contains(&pack(b'a', b'b', b'c')));
        assert!(t.contains(&pack(b'b', b'c', b'd')));
        assert_eq!(unpack(pack(b'a', b'b', b'c')), *b"abc");
    }

    #[test]
    fn short_inputs_have_none() {
        assert!(trigrams(b"ab", false).is_empty());
        assert!(trigrams(b"", true).is_empty());
    }

    #[test]
    fn folds_case() {
        assert_eq!(trigrams(b"ABC", true), trigrams(b"abc", true));
        assert_ne!(trigrams(b"ABC", false), trigrams(b"abc", false));
    }
}
