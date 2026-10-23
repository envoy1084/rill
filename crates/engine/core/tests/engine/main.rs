//! The core engine over memory stores: history, diff, merge, rebase,
//! trees and storage.

mod common;

mod ancestry;
mod criss_cross;
mod diff_properties;
mod graph_infos;
mod large_dirs;
mod marker_properties;
mod rebase;
mod storing;
mod streaming_memory;
mod structural;
mod tree_properties;
mod validation;
mod workflow;
