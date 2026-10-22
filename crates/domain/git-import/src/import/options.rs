//! Import options and the resulting report.

use rill_types::{ChangeVersionHash, TrunkSeq};

/// How an import behaves.
#[derive(Debug, Clone)]
pub struct ImportOptions {
    /// After importing, land every first-parent ancestor of the head on
    /// trunk in order, so trunk sequence numbers follow the mainline.
    pub land_first_parent_history: bool,
    /// Commits imported between two saves of the import map.
    pub save_every: usize,
    /// Commits buffered between the reader thread and the writer.
    pub read_ahead: usize,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            land_first_parent_history: true,
            save_every: 256,
            read_ahead: 8,
        }
    }
}

/// What an import did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Commits turned into new change versions.
    pub imported: usize,
    /// Commits already present in the import map.
    pub skipped: usize,
    /// Version of the requested head.
    pub head: Option<ChangeVersionHash>,
    /// Trunk head after landing, if landing was requested.
    pub trunk_head: Option<ChangeVersionHash>,
    /// Trunk sequence after landing, if landing was requested.
    pub seq: Option<TrunkSeq>,
    /// Versions landed by this run.
    pub landed: usize,
    /// Non-fatal problems, such as skipped submodules.
    pub warnings: Vec<String>,
}
