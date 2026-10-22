//! Structural merge for Rill: [`weave_core`]'s entity-level three-way
//! merge behind `rill_core`'s [`ContentMerger`], so a text conflict the
//! line merge leaves (two imports, match arms, enum variants, list entries
//! or config keys added side by side) is resolved by the file's syntax.
//!
//! Weave only ever sees files the line merge already conflicted on, and
//! anything short of a clean, warning-free entity merge declines: the
//! line conflict then stands, exactly as without this crate.

mod config;
mod merger;
mod stats;

pub use config::MergeConfig;
pub use merger::{WeaveMerger, WEAVE_VERSION};
pub use stats::{LanguageStats, MergeStats};
