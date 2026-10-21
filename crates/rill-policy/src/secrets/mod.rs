//! Secret scanning: credentials found in content as it enters a
//! repository. Pure: the server feeds it file content and a repository's
//! rules, and decides what a finding means.

mod detectors;
mod rules;
mod scan;

pub use detectors::{entropy, Detector, BUILT_IN};
pub use rules::{ScanMode, SecretRules, RULES_PATH};
pub use scan::{fingerprint, scan, Finding, MAX_FINDINGS};
