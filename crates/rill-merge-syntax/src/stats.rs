//! Counts of what structural merge did, by language (file extension).

use std::collections::BTreeMap;

use parking_lot::Mutex;

/// Outcomes for one extension.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LanguageStats {
    /// Conflicts weave resolved.
    pub resolved: u64,
    /// Conflicts left to the line merge (weave conflicted, warned or fell
    /// back to lines).
    pub declined: u64,
    /// Inputs it could not take (not UTF-8, too large).
    pub skipped: u64,
}

/// Outcomes by extension, kept for metrics.
#[derive(Default)]
pub struct MergeStats {
    by_extension: Mutex<BTreeMap<String, LanguageStats>>,
}

impl MergeStats {
    pub(crate) fn count(&self, extension: &str, f: impl FnOnce(&mut LanguageStats)) {
        f(self
            .by_extension
            .lock()
            .entry(extension.to_string())
            .or_default());
    }

    /// Every extension's counts so far.
    pub fn snapshot(&self) -> BTreeMap<String, LanguageStats> {
        self.by_extension.lock().clone()
    }
}
