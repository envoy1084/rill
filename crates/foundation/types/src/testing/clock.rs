//! Thread-local overrides for the wall clock and id generation, so a
//! single-threaded simulation produces the same timestamps and ids for the
//! same seed.

use std::cell::Cell;

thread_local! {
    static NOW: Cell<Option<i64>> = const { Cell::new(None) };
    static IDS: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Make [`Timestamp::now`](crate::Timestamp::now) on this thread return
/// `millis` instead of the wall clock; `None` restores the wall clock.
pub fn set_clock(millis: Option<i64>) {
    NOW.with(|now| now.set(millis));
}

/// Generate ids on this thread from `seed` instead of the OS random source;
/// `None` restores random ids.
pub fn set_id_seed(seed: Option<u64>) {
    IDS.with(|ids| ids.set(seed));
}

pub(crate) fn fixed_now() -> Option<i64> {
    NOW.with(Cell::get)
}

/// The next seeded id, if seeding is on (SplitMix64, two draws).
pub(crate) fn seeded_id() -> Option<[u8; 16]> {
    IDS.with(|ids| {
        let mut state = ids.get()?;
        let mut out = [0u8; 16];
        for half in out.chunks_mut(8) {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            half.copy_from_slice(&(z ^ (z >> 31)).to_le_bytes());
        }
        ids.set(Some(state));
        Some(out)
    })
}
