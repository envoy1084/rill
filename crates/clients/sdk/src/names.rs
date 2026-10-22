//! Resource names: the full names requests carry, built from what callers
//! hold, and the values read back out of the names responses carry.

use rill_proto::names;
use rill_types::{ChangeId, ChangeVersionHash, OperationId, RefName};

use crate::error::{Error, Result};

/// The org a repository is in when the caller names only the repository.
pub const DEFAULT_ORG: &str = "default";

/// `orgs/{org}/repos/{repo}` for what a caller calls a repository: a full
/// name, `org/repo`, or a bare repository name or id in the default org.
pub fn repo(spec: &str) -> String {
    if spec.starts_with("orgs/") {
        return spec.to_string();
    }
    match spec.split_once('/') {
        Some((org, repo)) => names::repo(org, repo),
        None => names::repo(DEFAULT_ORG, spec),
    }
}

/// `…/versions/{hash}` in `repo` (a full repository name).
pub fn version(repo: &str, hash: &ChangeVersionHash) -> String {
    format!("{repo}/versions/{hash}")
}

/// `…/changes/{change}` in `repo`.
pub fn change(repo: &str, id: &ChangeId) -> String {
    format!("{repo}/changes/{id}")
}

/// `…/intents/{intent}` in `repo`.
pub fn intent(repo: &str, id: &rill_types::IntentId) -> String {
    format!("{repo}/intents/{id}")
}

/// `…/refs/{ref}` in `repo`.
pub fn ref_name(repo: &str, name: &RefName) -> String {
    format!("{repo}/refs/{name}")
}

/// `…/policies/{policy}` of what a caller calls a repository.
pub fn policy(repo_spec: &str, policy: &str) -> String {
    format!("{}/policies/{policy}", repo(repo_spec))
}

/// `orgs/{org}`; the default org when `org` is empty.
pub fn org(org: &str) -> String {
    match org {
        "" => names::ORG.format(&[DEFAULT_ORG]),
        o if o.starts_with("orgs/") => o.to_string(),
        o => names::ORG.format(&[o]),
    }
}

/// The version hash a version name ends with.
pub fn version_hash(name: &str) -> Result<ChangeVersionHash> {
    let parts = names::VERSION
        .parse(name)
        .map_err(|e| Error::config(e.to_string()))?;
    Ok(parts[2].parse()?)
}

/// The short name (`release/1.0`) of a ref resource name.
pub fn ref_short(name: &str) -> Result<RefName> {
    let parts = names::REF
        .parse(name)
        .map_err(|e| Error::config(e.to_string()))?;
    Ok(RefName::new(parts[2])?)
}

/// The `request_id` for an idempotency key: the key as a UUID.
pub fn request_id(op: &OperationId) -> String {
    uuid::Uuid::from_bytes(*op.as_bytes()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repositories_are_named_in_full_whatever_the_caller_holds() {
        assert_eq!(repo("mono"), "orgs/default/repos/mono");
        assert_eq!(repo("acme/mono"), "orgs/acme/repos/mono");
        assert_eq!(repo("orgs/acme/repos/mono"), "orgs/acme/repos/mono");
    }
}
