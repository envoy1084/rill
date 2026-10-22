//! The action cache and what guards it: sealed entries, referenced
//! content checks, and hermeticity probes.

mod nonhermetic;
pub(crate) mod probe;
mod results;

pub use nonhermetic::{Nonhermetic, NONHERMETIC_FOR};
pub use probe::differences;
