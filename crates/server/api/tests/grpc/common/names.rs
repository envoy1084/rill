//! Resource names tests address requests with.

use rill_types::{ChangeId, ObjectHash};

/// `orgs/{org}/repos/{repo}` for `main`, `acme/main` or a full name.
pub(crate) fn repo(spec: &str) -> String {
    if spec.starts_with("orgs/") {
        return spec.to_string();
    }
    match spec.split_once('/') {
        Some((org, name)) => format!("orgs/{org}/repos/{name}"),
        None => format!("orgs/default/repos/{spec}"),
    }
}

/// `orgs/{org}`.
pub(crate) fn org(org: &str) -> String {
    format!("orgs/{org}")
}

/// `…/changes/{change}` for a change id as the wire carries it.
pub(crate) fn change(repo_spec: &str, change_id: &[u8]) -> String {
    let id = ChangeId::from_bytes(change_id.try_into().expect("a 16-byte change id"));
    format!("{}/changes/{id}", repo(repo_spec))
}

/// `…/changes/{change}/review`.
pub(crate) fn review(repo_spec: &str, change_id: &[u8]) -> String {
    format!("{}/review", change(repo_spec, change_id))
}

/// `…/versions/{hash}` for a version hash as the wire carries it.
pub(crate) fn version(repo_spec: &str, hash: &[u8]) -> String {
    let hash = ObjectHash::from_slice(hash).expect("a 32-byte hash");
    format!("{}/versions/{}", repo(repo_spec), hash.to_hex())
}

/// `…/trunk/positions/{n}`.
pub(crate) fn position(repo_spec: &str, n: u64) -> String {
    format!("{}/trunk/positions/{n}", repo(repo_spec))
}

/// A fresh idempotency key.
pub(crate) fn request_id() -> String {
    uuid::Uuid::new_v4().to_string()
}
