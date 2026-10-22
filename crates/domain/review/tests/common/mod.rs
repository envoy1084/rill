//! Shared review fixture: a service over in-memory stores.

#![allow(dead_code, unused_imports)]

mod harness;
mod sources;

pub use harness::{harness, harness_on, Harness};
pub use sources::{LIB_V0, LIB_V1};
