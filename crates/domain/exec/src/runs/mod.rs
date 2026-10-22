//! Verification runs: what each verification ran, step by step, kept so
//! its progress, logs and artifacts can be read while it runs and after.

mod record;
mod store;

pub use record::{
    IndexedStep, Run, RunEnd, RunHeader, StepEnd, StepRecord, StepState, StoredDigest,
};
pub use store::{RunHandle, RunStore};
