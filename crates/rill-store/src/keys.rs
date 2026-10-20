//! Key layout of the metadata store.
//!
//! Every key is `<table>/<repo-id>/<...>`. Tables are short ASCII tags so a
//! prefix scan over a repo's rows in one table is cheap, and deleting a repo
//! is a prefix delete per table.

use bytes::Bytes;
use rill_types::{ChangeId, IntentId, ObjectHash, OperationId, RefName, RepoId, WorkspaceId};

use crate::meta::seq_key;

/// Record tables.
pub mod table {
    /// `ci/<repo>/<hash>` → encoded ChangeInfo
    pub const CHANGE_INFO: &str = "ci";
    /// `sig/<repo>/<hash>` → encoded Vec<Signature>
    pub const SIGNATURES: &str = "sig";
    /// `ch/<repo>/<change-id>` → encoded ReviewState
    pub const CHANGE: &str = "ch";
    /// `log/<repo>/<be64 seq>` → version hash (the write-ahead log)
    pub const LOG: &str = "log";
    /// `trunk/<repo>/<be64 seq>` → version hash
    pub const TRUNK: &str = "trunk";
    /// `trunkh/<repo>/<hash>` → be64 seq
    pub const TRUNK_BY_HASH: &str = "trunkh";
    /// `ref/<repo>/<name>` → version hash
    pub const REF: &str = "ref";
    /// `ws/<repo>/<workspace-id>` → encoded Workspace
    pub const WORKSPACE: &str = "ws";
    /// `op/<repo>/<workspace-id>/<be64 seq>` → encoded Operation
    pub const OPERATION: &str = "op";
    /// `opi/<repo>/<operation-id>` → be64 seq (idempotency lookup)
    pub const OPERATION_INDEX: &str = "opi";
    /// `in/<repo>/<intent-id>` → encoded Intent
    pub const INTENT: &str = "in";
    /// `dd/<repo>/<type>/<hash>` → derived data bytes
    pub const DERIVED: &str = "dd";
    /// `chv/<repo>/<change-id>/<be64 seq>` → version hash, versions in order
    pub const CHANGE_VERSIONS: &str = "chv";
    /// `repo/<repo-id>` → encoded RepoConfig
    pub const REPO: &str = "repo";
    /// `repon/<name>` → repo id
    pub const REPO_BY_NAME: &str = "repon";
    /// `rebac/<repo>/<object>/<relation>/<subject>` → empty
    pub const REBAC: &str = "rebac";
    /// `misc/<repo>/<name>` → arbitrary small values (policy, settings)
    pub const MISC: &str = "misc";
}

fn join(parts: &[&[u8]]) -> Bytes {
    let len: usize = parts.iter().map(|p| p.len() + 1).sum();
    let mut out = Vec::with_capacity(len);
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push(b'/');
        }
        out.extend_from_slice(p);
    }
    Bytes::from(out)
}

/// Prefix of a repo's rows in a table, including the trailing slash.
pub fn repo_prefix(table: &str, repo: &RepoId) -> Bytes {
    let mut out = join(&[table.as_bytes(), repo.as_bytes()]).to_vec();
    out.push(b'/');
    Bytes::from(out)
}

pub fn change_info(repo: &RepoId, hash: &ObjectHash) -> Bytes {
    join(&[
        table::CHANGE_INFO.as_bytes(),
        repo.as_bytes(),
        hash.as_bytes(),
    ])
}

pub fn signatures(repo: &RepoId, hash: &ObjectHash) -> Bytes {
    join(&[
        table::SIGNATURES.as_bytes(),
        repo.as_bytes(),
        hash.as_bytes(),
    ])
}

pub fn change(repo: &RepoId, id: &ChangeId) -> Bytes {
    join(&[table::CHANGE.as_bytes(), repo.as_bytes(), id.as_bytes()])
}

pub fn change_versions_prefix(repo: &RepoId, id: &ChangeId) -> Bytes {
    let mut out = join(&[
        table::CHANGE_VERSIONS.as_bytes(),
        repo.as_bytes(),
        id.as_bytes(),
    ])
    .to_vec();
    out.push(b'/');
    Bytes::from(out)
}

pub fn log_prefix(repo: &RepoId) -> Bytes {
    repo_prefix(table::LOG, repo)
}

pub fn trunk_prefix(repo: &RepoId) -> Bytes {
    repo_prefix(table::TRUNK, repo)
}

pub fn trunk(repo: &RepoId, seq: u64) -> Bytes {
    seq_key(&trunk_prefix(repo), seq)
}

pub fn trunk_by_hash(repo: &RepoId, hash: &ObjectHash) -> Bytes {
    join(&[
        table::TRUNK_BY_HASH.as_bytes(),
        repo.as_bytes(),
        hash.as_bytes(),
    ])
}

pub fn reference(repo: &RepoId, name: &RefName) -> Bytes {
    join(&[
        table::REF.as_bytes(),
        repo.as_bytes(),
        name.as_str().as_bytes(),
    ])
}

pub fn ref_prefix(repo: &RepoId) -> Bytes {
    repo_prefix(table::REF, repo)
}

pub fn workspace(repo: &RepoId, id: &WorkspaceId) -> Bytes {
    join(&[table::WORKSPACE.as_bytes(), repo.as_bytes(), id.as_bytes()])
}

pub fn workspace_prefix(repo: &RepoId) -> Bytes {
    repo_prefix(table::WORKSPACE, repo)
}

pub fn operation_prefix(repo: &RepoId, ws: &WorkspaceId) -> Bytes {
    let mut out = join(&[table::OPERATION.as_bytes(), repo.as_bytes(), ws.as_bytes()]).to_vec();
    out.push(b'/');
    Bytes::from(out)
}

pub fn operation_index(repo: &RepoId, op: &OperationId) -> Bytes {
    join(&[
        table::OPERATION_INDEX.as_bytes(),
        repo.as_bytes(),
        op.as_bytes(),
    ])
}

pub fn intent(repo: &RepoId, id: &IntentId) -> Bytes {
    join(&[table::INTENT.as_bytes(), repo.as_bytes(), id.as_bytes()])
}

pub fn intent_prefix(repo: &RepoId) -> Bytes {
    repo_prefix(table::INTENT, repo)
}

pub fn derived(repo: &RepoId, kind: &str, hash: &ObjectHash) -> Bytes {
    join(&[
        table::DERIVED.as_bytes(),
        repo.as_bytes(),
        kind.as_bytes(),
        hash.as_bytes(),
    ])
}

/// The store-wide format marker.
pub fn store_format() -> Bytes {
    Bytes::from_static(b"meta/format")
}

pub fn repo(id: &RepoId) -> Bytes {
    join(&[table::REPO.as_bytes(), id.as_bytes()])
}

pub fn repo_by_name(name: &str) -> Bytes {
    join(&[table::REPO_BY_NAME.as_bytes(), name.as_bytes()])
}

pub fn repo_list_prefix() -> Bytes {
    Bytes::from(format!("{}/", table::REPO))
}

pub fn rebac(repo: &RepoId, object: &str, relation: &str, subject: &str) -> Bytes {
    join(&[
        table::REBAC.as_bytes(),
        repo.as_bytes(),
        object.as_bytes(),
        relation.as_bytes(),
        subject.as_bytes(),
    ])
}

pub fn rebac_object_prefix(repo: &RepoId, object: &str) -> Bytes {
    let mut out = join(&[table::REBAC.as_bytes(), repo.as_bytes(), object.as_bytes()]).to_vec();
    out.push(b'/');
    Bytes::from(out)
}

pub fn misc(repo: &RepoId, name: &str) -> Bytes {
    join(&[table::MISC.as_bytes(), repo.as_bytes(), name.as_bytes()])
}

/// Extract the trailing object hash from a key like `cv/<repo>/<hash>`.
pub fn hash_suffix(key: &[u8]) -> Option<ObjectHash> {
    if key.len() < 32 {
        return None;
    }
    ObjectHash::from_slice(&key[key.len() - 32..]).ok()
}
