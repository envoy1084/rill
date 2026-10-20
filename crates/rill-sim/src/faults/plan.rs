use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use parking_lot::Mutex;

/// What happens to one store call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    None,
    /// The call fails without effect.
    Fail,
    /// The write takes effect but the caller sees an error, as when a
    /// process dies between committing and replying.
    LoseAck,
}

/// Decides which store operations fail.
///
/// A decision is a hash of the seed, the operation, the keys it touches and
/// how many times that same operation has run, not a draw from a shared
/// sequence. Code that iterates hash sets issues store calls in a different
/// order from run to run; keyed decisions keep a seed's faults identical
/// regardless.
pub struct FaultPlan {
    seed: u64,
    fail_rate: f64,
    lose_ack_rate: f64,
    enabled: AtomicBool,
    injected: AtomicU64,
    calls: Mutex<HashMap<u64, u64>>,
}

impl FaultPlan {
    /// A plan for `seed`; starts disabled so setup is fault-free.
    pub fn new(seed: u64, fail_rate: f64, lose_ack_rate: f64) -> Self {
        FaultPlan {
            seed,
            fail_rate,
            lose_ack_rate,
            enabled: AtomicBool::new(false),
            injected: AtomicU64::new(0),
            calls: Mutex::new(HashMap::new()),
        }
    }

    /// Start or stop injecting faults.
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::SeqCst);
    }

    /// The fate of a write identified by `op`.
    pub fn write(&self, op: impl Hash) -> Fault {
        let Some(roll) = self.roll(("write", op)) else {
            return Fault::None;
        };
        let fault = if roll < self.fail_rate {
            Fault::Fail
        } else if roll < self.fail_rate + self.lose_ack_rate {
            Fault::LoseAck
        } else {
            Fault::None
        };
        if fault != Fault::None {
            self.injected.fetch_add(1, Ordering::SeqCst);
        }
        fault
    }

    /// Whether a read identified by `op` fails.
    pub fn read_fails(&self, op: impl Hash) -> bool {
        let failed = self
            .roll(("read", op))
            .is_some_and(|roll| roll < self.fail_rate / 4.0);
        if failed {
            self.injected.fetch_add(1, Ordering::SeqCst);
        }
        failed
    }

    /// Faults injected so far.
    pub fn injected(&self) -> u64 {
        self.injected.load(Ordering::SeqCst)
    }

    /// A uniform value in `[0, 1)` for this occurrence of `op`, or `None`
    /// while disabled.
    fn roll(&self, op: impl Hash) -> Option<f64> {
        if !self.enabled.load(Ordering::SeqCst) {
            return None;
        }
        let mut h = DefaultHasher::new();
        op.hash(&mut h);
        let id = h.finish();
        let nth = {
            let mut calls = self.calls.lock();
            let n = calls.entry(id).or_insert(0);
            *n += 1;
            *n
        };
        let mut h = DefaultHasher::new();
        (self.seed, id, nth).hash(&mut h);
        Some((h.finish() >> 11) as f64 / (1u64 << 53) as f64)
    }
}

pub(crate) fn injected_error(what: &str) -> rill_store::Error {
    rill_store::Error::Backend(format!("injected fault: {what}"))
}
