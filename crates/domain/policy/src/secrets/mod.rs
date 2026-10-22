//! Secret scanning: credentials found in content as it enters a
//! repository. Pure: the server feeds it file content and a repository's
//! rules, and decides what a finding means.

mod catalog;
mod entropy;
mod rules;
mod scan;

pub use catalog::{Catalog, Detector, CATALOG};
pub use entropy::entropy;
pub use rules::{ScanMode, SecretRules, RULES_PATH};
pub use scan::{fingerprint, scan, Finding, MAX_FINDINGS};
