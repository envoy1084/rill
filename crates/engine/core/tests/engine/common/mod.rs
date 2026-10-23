//! Shared fixture for rill-core integration tests.

#![allow(dead_code, unused_imports)]

mod fixture;
mod values;

pub use fixture::{content_at, fixture, Fixture};
pub use values::{p, t};
