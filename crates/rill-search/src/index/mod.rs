//! The search index: per-blob trigram postings plus per-root path
//! manifests, both persisted as derived data of the repository.

mod config;
mod docs;
mod manifest;
mod persist;
mod search_index;
mod sync;

pub use config::IndexConfig;
pub use docs::DocTable;
pub use manifest::{ManifestEntry, RootManifest};
pub use persist::{KIND_BLOB, KIND_DOCS, KIND_POST, KIND_ROOT, KIND_STATE};
pub use search_index::{FlushStats, IndexStats, SearchIndex};
