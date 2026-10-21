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
    /// `(evt, ns, repo, stamp)` → Event
    pub const EVENT: &str = "evt";
    /// `(evtn, ns, repo)` → counter of events written; tailers wait on it
    pub const EVENT_COUNT: &str = "evtn";
    /// `(thd, ns, repo)` → be64 seq ++ version hash of the trunk head
    pub const TRUNK_HEAD: &str = "thd";
    /// `(land, ns, repo, seq)` → Landing
    pub const LANDING: &str = "land";
    /// `(lease, ns, repo, role)` → LeaseRecord (holder, expiry)
    pub const LEASE: &str = "lease";
    /// `(lepoch, ns, repo, role)` → be64 epoch; fenced writes check it
    pub const LEASE_EPOCH: &str = "lepoch";
    /// `(lrt, ns, repo, ticket)` → LandRequestRecord
    pub const LAND_REQUEST: &str = "lrt";
    /// `(lrq, ns, repo, lane, requested_at, ticket)` → empty; requests not
    /// yet finished, in the order they are served
    pub const LAND_PENDING: &str = "lrq";
    /// `(lrc, ns, repo, change)` → be64 ticket of the change's unfinished
    /// request
    pub const LAND_BY_CHANGE: &str = "lrc";
    /// `(lrn, ns, repo)` → counter moved by every new request
    pub const LAND_WAKE: &str = "lrn";
    /// `(jobq, ns, repo, queue, shard, priority, run_at, stamp)` → job ready
    /// to run from `run_at` on
    pub const JOB_READY: &str = "jobq";
    /// `(jobl, ns, repo, queue, job)` → claimed job with its lease
    pub const JOB_LEASED: &str = "jobl";
    /// `(jobt, ns, repo, queue, job)` → the claim's token; fenced writes
    /// check it
    pub const JOB_TOKEN: &str = "jobt";
    /// `(jobd, ns, repo, queue, job)` → job that ran out of attempts
    pub const JOB_DEAD: &str = "jobd";
    /// `(jobn, ns, repo, queue, metric)` → queue counter
    pub const JOB_COUNT: &str = "jobn";
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
    /// `(idem, principal, operation)` → the request's claim or response
    pub const IDEM: &str = "idem";
    /// `(idl, stamp)` → IdentityEvent
    pub const IDENT_LOG: &str = "idl";
    /// `(idln)` → identity log counter followers wait on
    pub const IDENT_LOG_COUNT: &str = "idln";
    /// `(idk, key id)` → TokenKeyRecord
    pub const IDENT_KEY: &str = "idk";
    /// `(idss, session)` → SessionRecord
    pub const IDENT_SESSION: &str = "idss";
    /// `(idrv, token)` → Revocation
    pub const IDENT_REVOKED: &str = "idrv";
    /// `(idu, org, name)` → UserRecord
    pub const IDENT_USER: &str = "idu";
    /// `(idpn, kind, name)` → the org a principal name belongs to
    pub const IDENT_PRINCIPAL: &str = "idpn";
    /// `(idx, issuer, subject)` → `(org, name)` of the user it signs in as
    pub const IDENT_EXTERNAL: &str = "idx";
    /// `(idp, org, name)` → IdentityProvider
    pub const IDENT_PROVIDER: &str = "idp";
    /// `(idpi, issuer)` → `(org, name)` of the provider with that issuer
    pub const IDENT_PROVIDER_ISSUER: &str = "idpi";
    /// `(idpl, state)` → PendingLogin
    pub const IDENT_LOGIN: &str = "idpl";
    /// `(ida, org, name)` → AgentRecord
    pub const IDENT_AGENT: &str = "ida";
    /// `(idw, org, name)` → WorkloadIssuer
    pub const IDENT_WORKLOAD: &str = "idw";
    /// `(idos, org)` → OrgIdentitySettings
    pub const IDENT_ORG_SETTINGS: &str = "idos";
    /// `(idb, principal, metric, day)` → what the principal spent that day
    pub const IDENT_BUDGET: &str = "idb";
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

    pub fn trunk_head(&self) -> Bytes {
        self.table(table::TRUNK_HEAD)
    }

    pub fn landings(&self) -> Bytes {
        self.table(table::LANDING)
    }

    pub fn landing(&self, seq: u64) -> Bytes {
        seq_key(&self.landings(), seq)
    }

    pub fn lease(&self, role: &str) -> Bytes {
        self.row(table::LEASE, &role)
    }

    pub fn lease_epoch(&self, role: &str) -> Bytes {
        self.row(table::LEASE_EPOCH, &role)
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

    pub fn land_request(&self, ticket: u64) -> Bytes {
        self.row(table::LAND_REQUEST, &(ticket as i64))
    }

    pub fn land_requests(&self) -> Bytes {
        self.table(table::LAND_REQUEST)
    }

    pub fn land_pending(&self, lane: u8, requested_at: i64, ticket: u64) -> Bytes {
        self.row(
            table::LAND_PENDING,
            &(i64::from(lane), requested_at, ticket as i64),
        )
    }

    pub fn land_pending_all(&self) -> Bytes {
        self.table(table::LAND_PENDING)
    }

    pub fn land_by_change(&self, change: &ChangeId) -> Bytes {
        self.row(table::LAND_BY_CHANGE, &change.as_bytes().as_slice())
    }

    /// Jobs of `queue` in `shard` at `priority`, ready or not yet.
    pub fn jobs_ready(&self, queue: &str, shard: u8, priority: u8) -> Bytes {
        self.row(
            table::JOB_READY,
            &(queue, i64::from(shard), i64::from(priority)),
        )
    }

    /// The prefix a job ready from `run_at` is stamped under; also the
    /// exclusive end of the jobs ready before `run_at`.
    pub fn job_ready_at(&self, queue: &str, shard: u8, priority: u8, run_at: i64) -> Bytes {
        self.row(
            table::JOB_READY,
            &(queue, i64::from(shard), i64::from(priority), run_at),
        )
    }

    pub fn jobs_leased(&self, queue: &str) -> Bytes {
        self.row(table::JOB_LEASED, &queue)
    }

    pub fn job_leased(&self, queue: &str, job: &[u8; 16]) -> Bytes {
        self.row(table::JOB_LEASED, &(queue, job.as_slice()))
    }

    pub fn job_token(&self, queue: &str, job: &[u8; 16]) -> Bytes {
        self.row(table::JOB_TOKEN, &(queue, job.as_slice()))
    }

    pub fn jobs_dead(&self, queue: &str) -> Bytes {
        self.row(table::JOB_DEAD, &queue)
    }

    pub fn job_dead(&self, queue: &str, job: &[u8; 16]) -> Bytes {
        self.row(table::JOB_DEAD, &(queue, job.as_slice()))
    }

    pub fn job_count(&self, queue: &str, metric: &str) -> Bytes {
        self.row(table::JOB_COUNT, &(queue, metric))
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
