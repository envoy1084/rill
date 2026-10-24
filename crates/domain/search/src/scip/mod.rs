//! SCIP code-intelligence indexes: precise definitions and references a
//! repository's CI uploads, kept per root and trusted for a file only
//! while its content is what the index was built from.

mod index;
mod store;

pub use index::{ScipDocument, ScipIndex, ScipOccurrence};
pub(crate) use store::ScipCache;
pub use store::{scip_for, store_scip, ScipLocation, ScipView};
