//! Derived data: values computed from a change version and cached in the
//! metadata store, with built-in derivables for trees, generations, change
//! summaries, path history and blame.

mod blame;
mod ctx;
mod generation;
mod history;
mod manager;
mod root_tree;
mod summary;
mod traits;

pub use blame::{Blame, BlameLine};
pub use ctx::DeriveCtx;
pub use generation::Generation;
pub use history::{HistoryEntry, HistoryNode, PathHistory};
pub use manager::DeriveManager;
pub use root_tree::RootTree;
pub use summary::{ChangeSummary, SummaryKind};
pub use traits::{derived_kind_key, Derivable};
