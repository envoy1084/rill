//! Key layout of the metadata store.
//!
//! Every key is a FoundationDB tuple. Repository rows are
//! `(table, namespace, repo, ...)`, so a repository's rows in one table form
//! one range and a namespace's rows a larger one. Tuple elements are
//! self-delimiting: no name or id is ever a prefix of a longer one's key.

use bytes::Bytes;
use foundationdb_tuple::{pack, unpack, TuplePack, TupleUnpack};
use rill_types::{
    ChangeId, IntentId, ObjectHash, OperationId, RefName, RepoId, Versionstamp, WorkspaceId,
};

use crate::error::{Error, Result};
use crate::meta::{seq_key, stamp_key};

/// Table names, the first element of every key.
pub mod table {
    /// `(ci, ns, repo, hash)` → ChangeInfo
    pub const CHANGE_INFO: &str = "ci";
    /// `(sig, ns, repo, hash)` → signatures
    pub const SIGNATURES: &str = "sig";
    /// `(ch, ns, repo, change)` → ChangeRecord
    pub const CHANGE: &str = "ch";
    /// `(vote, ns, repo, change, label, voter, version)` → LabelVote
    pub const VOTE: &str = "vote";
    /// `(vep, ns, repo, change)` → changes whenever a vote is written
    pub const VOTE_EPOCH: &str = "vep";
    /// `(thr, ns, repo, change, thread)` → ThreadHeader
    pub const THREAD: &str = "thr";
    /// `(cmt, ns, repo, change, thread, stamp)` → Comment
    pub const COMMENT: &str = "cmt";
    /// `(log, ns, repo, stamp)` → version hash, every stored version
    pub const LOG: &str = "log";
    /// `(trunk, ns, repo, seq)` → version hash
    pub const TRUNK: &str = "trunk";
    /// `(trunkh, ns, repo, hash)` → be64 seq
    pub const TRUNK_BY_HASH: &str = "trunkh";
    /// `(ref, ns, repo, name)` → version hash
    pub const REF: &str = "ref";
    /// `(ws, ns, repo, workspace)` → Workspace
    pub const WORKSPACE: &str = "ws";
    /// `(op, ns, repo, workspace, seq)` → Operation
    pub const OPERATION: &str = "op";
    /// `(opi, ns, repo, operation)` → empty; idempotency lookup
    pub const OPERATION_INDEX: &str = "opi";
    /// `(in, ns, repo, intent)` → Intent
    pub const INTENT: &str = "in";
    /// `(dd, ns, repo, kind, hash)` → derived value
    pub const DERIVED: &str = "dd";
    /// `(chv, ns, repo, change, seq)` → version hash, versions in order
    pub const CHANGE_VERSIONS: &str = "chv";
    /// `(repo, ns, repo)` → RepoConfig
    pub const REPO: &str = "repo";
    /// `(repon, ns, name)` → repo id
    pub const REPO_BY_NAME: &str = "repon";
    /// `(rebac, ns, repo, object, relation, subject)` → empty
    pub const REBAC: &str = "rebac";
    /// `(misc, ns, repo, name)` → small values (policy, rings, cursors)
    pub const MISC: &str = "misc";
    /// `(meta, "format")` → store format
    pub const META: &str = "meta";
    /// `(org, name)` → OrgConfig
    pub const ORG: &str = "org";
    /// `(repoid, repo)` → the org a repository belongs to
    pub const REPO_ORG: &str = "repoid";
    /// `(quota, org, principal, metric, day, shard)` → usage counter
    pub const QUOTA: &str = "quota";
}

fn key<T: TuplePack>(t: &T) -> Bytes {
    Bytes::from(pack(t))
}

/// Decode the tuple that follows `prefix` in `key`.
pub fn tail<'de, T: TupleUnpack<'de>>(prefix: &[u8], key: &'de [u8]) -> Result<T> {
    let rest = key
        .strip_prefix(prefix)
        .ok_or_else(|| Error::Codec("key outside its table".into()))?;
    unpack(rest).map_err(|e| Error::Codec(format!("malformed key: {e}")))
}

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

pub fn repo(ns: &str, id: &RepoId) -> Bytes {
    key(&(table::REPO, ns, id.as_bytes().as_slice()))
}

pub fn repo_list(ns: &str) -> Bytes {
    key(&(table::REPO, ns))
}

pub fn repo_by_name(ns: &str, name: &str) -> Bytes {
    key(&(table::REPO_BY_NAME, ns, name))
}

/// The keys of one repository.
#[derive(Debug, Clone)]
pub struct RepoKeys {
    ns: String,
    repo: RepoId,
}

impl RepoKeys {
    pub fn new(ns: &str, repo: RepoId) -> Self {
        RepoKeys {
            ns: ns.to_string(),
            repo,
        }
    }

    /// The prefix of this repository's rows in `table`.
    pub fn table(&self, table: &str) -> Bytes {
        key(&(table, self.ns.as_str(), self.repo.as_bytes().as_slice()))
    }

    fn row<T: TuplePack>(&self, table: &str, rest: &T) -> Bytes {
        let mut out = self.table(table).to_vec();
        foundationdb_tuple::pack_into(rest, &mut out);
        Bytes::from(out)
    }

    pub fn change_info(&self, hash: &ObjectHash) -> Bytes {
        self.row(table::CHANGE_INFO, &hash.as_bytes().as_slice())
    }

    pub fn signatures(&self, hash: &ObjectHash) -> Bytes {
        self.row(table::SIGNATURES, &hash.as_bytes().as_slice())
    }

    pub fn change(&self, id: &ChangeId) -> Bytes {
        self.row(table::CHANGE, &id.as_bytes().as_slice())
    }

    pub fn votes(&self, id: &ChangeId) -> Bytes {
        self.row(table::VOTE, &id.as_bytes().as_slice())
    }

    pub fn vote(&self, id: &ChangeId, label: &str, voter: &str, version: &ObjectHash) -> Bytes {
        self.row(
            table::VOTE,
            &(
                id.as_bytes().as_slice(),
                label,
                voter,
                version.as_bytes().as_slice(),
            ),
        )
    }

    pub fn vote_epoch(&self, id: &ChangeId) -> Bytes {
        self.row(table::VOTE_EPOCH, &id.as_bytes().as_slice())
    }

    pub fn threads(&self, id: &ChangeId) -> Bytes {
        self.row(table::THREAD, &id.as_bytes().as_slice())
    }

    pub fn thread(&self, id: &ChangeId, thread: &str) -> Bytes {
        self.row(table::THREAD, &(id.as_bytes().as_slice(), thread))
    }

    pub fn comments(&self, id: &ChangeId, thread: &str) -> Bytes {
        self.row(table::COMMENT, &(id.as_bytes().as_slice(), thread))
    }

    pub fn change_versions(&self, id: &ChangeId) -> Bytes {
        self.row(table::CHANGE_VERSIONS, &id.as_bytes().as_slice())
    }

    pub fn log(&self) -> Bytes {
        self.table(table::LOG)
    }

    pub fn log_entry(&self, stamp: &Versionstamp) -> Bytes {
        stamp_key(&self.log(), stamp)
    }

    pub fn trunk_table(&self) -> Bytes {
        self.table(table::TRUNK)
    }

    pub fn trunk(&self, seq: u64) -> Bytes {
        seq_key(&self.trunk_table(), seq)
    }

    pub fn trunk_by_hash(&self, hash: &ObjectHash) -> Bytes {
        self.row(table::TRUNK_BY_HASH, &hash.as_bytes().as_slice())
    }

    pub fn reference(&self, name: &RefName) -> Bytes {
        self.row(table::REF, &name.as_str())
    }

    pub fn workspace(&self, id: &WorkspaceId) -> Bytes {
        self.row(table::WORKSPACE, &id.as_bytes().as_slice())
    }

    pub fn operations(&self, ws: &WorkspaceId) -> Bytes {
        self.row(table::OPERATION, &ws.as_bytes().as_slice())
    }

    pub fn operation_index(&self, op: &OperationId) -> Bytes {
        self.row(table::OPERATION_INDEX, &op.as_bytes().as_slice())
    }

    pub fn intent(&self, id: &IntentId) -> Bytes {
        self.row(table::INTENT, &id.as_bytes().as_slice())
    }

    pub fn derived(&self, kind: &str, hash: &ObjectHash) -> Bytes {
        self.row(table::DERIVED, &(kind, hash.as_bytes().as_slice()))
    }

    pub fn relation(&self, object: &str, relation: &str, subject: &str) -> Bytes {
        self.row(table::REBAC, &(object, relation, subject))
    }

    pub fn relations_of(&self, object: &str) -> Bytes {
        self.row(table::REBAC, &object)
    }

    pub fn misc(&self, name: &str) -> Bytes {
        self.row(table::MISC, &name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::KeyRange;

    #[test]
    fn a_name_never_prefixes_a_longer_names_rows() {
        let keys = RepoKeys::new("default", RepoId::from_bytes([7; 16]));
        let short = KeyRange::prefix(keys.relations_of("dir:/src"));
        assert!(short.contains(&keys.relation("dir:/src", "owner", "alice")));
        assert!(!short.contains(&keys.relation("dir:/src/x", "owner", "alice")));
        let other = RepoKeys::new("default", RepoId::from_bytes([8; 16]));
        assert!(
            !KeyRange::prefix(keys.table(table::REF)).contains(&other.reference(&RefName::trunk()))
        );
    }
}
