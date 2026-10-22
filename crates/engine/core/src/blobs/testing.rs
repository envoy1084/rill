//! Deterministic incompressible bytes for content tests.

use bytes::Bytes;

pub(super) fn pseudo_random(len: usize, seed: u64) -> Bytes {
    let mut state = 0x9E37_79B9_7F4A_7C15u64 ^ seed;
    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        out.push(state as u8);
    }
    Bytes::from(out)
}
