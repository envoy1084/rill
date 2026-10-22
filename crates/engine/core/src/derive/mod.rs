//! Derived data: values computed from a change version and cached in the
//! metadata store, with built-in derivables for trees, ancestry, changed
//! paths, change summaries, path history and blame.

mod ancestry;
mod blame;
mod changed_paths;
mod ctx;
mod history;
mod land_critical;
mod manager;
mod registry;
mod root_tree;
mod summary;
mod traits;

pub use ancestry::{Ancestry, AncestryInfo};
pub use blame::{Blame, BlameLine};
pub use changed_paths::{ChangedPathSet, ChangedPaths};
pub use ctx::DeriveCtx;
pub use history::{HistoryEntry, HistoryNode, PathHistory};
pub use land_critical::LandCritical;
pub use manager::DeriveManager;
pub use registry::{kinds, scheduled, KindSpec};
pub use root_tree::RootTree;
pub use summary::{ChangeSummary, SummaryKind};
pub use traits::{derived_kind_key, Derivable, Schedule};
