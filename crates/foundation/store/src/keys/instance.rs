//! Keys of instance-wide and namespace-wide rows.

use bytes::Bytes;
use foundationdb_tuple::TuplePack;
use rill_types::RepoId;

use super::table;
use super::tuple::key;

/// The store-wide format marker.
pub fn store_format() -> Bytes {
    key(&(table::META, "format"))
}

pub fn org(name: &str) -> Bytes {
    key(&(table::ORG, name))
}

pub fn org_list() -> Bytes {
    key(&(table::ORG,))
}

pub fn repo_org(id: &RepoId) -> Bytes {
    key(&(table::REPO_ORG, id.as_bytes().as_slice()))
}

pub fn quota(org: &str, principal: &str, metric: &str, day: i64, shard: u8) -> Bytes {
    key(&(table::QUOTA, org, principal, metric, day, shard as u64))
}

pub fn idem(principal: &str, operation: &[u8]) -> Bytes {
    key(&(table::IDEM, principal, operation))
}

pub fn idem_table() -> Bytes {
    key(&(table::IDEM,))
}

/// A whole instance-wide identity table.
pub fn identity_table(table: &str) -> Bytes {
    key(&(table,))
}

/// One row of an instance-wide identity table keyed by `id`, or the
/// prefix of every row whose key starts with `id`.
pub fn identity_row<T: TuplePack>(table: &str, id: T) -> Bytes {
    key(&(table, id))
}

/// One row of an identity table keyed by two elements, such as
/// `(org, name)`; [`identity_row`] with the first is its prefix.
pub fn identity_pair<A: TuplePack, B: TuplePack>(table: &str, a: A, b: B) -> Bytes {
    key(&(table, a, b))
}

pub fn repo(ns: &str, id: &RepoId) -> Bytes {
    key(&(table::REPO, ns, id.as_bytes().as_slice()))
}

pub fn repo_list(ns: &str) -> Bytes {
    key(&(table::REPO, ns))
}

pub fn repo_by_name(ns: &str, name: &str) -> Bytes {
    key(&(table::REPO_BY_NAME, ns, name))
}
