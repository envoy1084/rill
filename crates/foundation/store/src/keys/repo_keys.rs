//! Keys of the rows of one repository.

use bytes::Bytes;
use foundationdb_tuple::TuplePack;
use rill_types::{
    ChangeId, IntentId, ObjectHash, OperationId, RefName, RepoId, Versionstamp, WorkspaceId,
};

use super::table;
use super::tuple::key;
use crate::meta::{seq_key, stamp_key};

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

    pub(super) fn row<T: TuplePack>(&self, table: &str, rest: &T) -> Bytes {
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

    pub fn relation_by_subject(&self, subject: &str, object: &str, relation: &str) -> Bytes {
        self.row(table::REBAC_BY_SUBJECT, &(subject, object, relation))
    }

    pub fn relations_of_subject(&self, subject: &str) -> Bytes {
        self.row(table::REBAC_BY_SUBJECT, &subject)
    }

    pub fn quarantine(&self, version: &ObjectHash) -> Bytes {
        self.row(table::QUARANTINE, &version.as_bytes().as_slice())
    }

    pub fn misc(&self, name: &str) -> Bytes {
        self.row(table::MISC, &name)
    }

    pub fn exec_operation(&self, operation: &str) -> Bytes {
        self.row(table::EXEC_OPERATION, &operation)
    }

    pub fn exec_operation_count(&self, operation: &str) -> Bytes {
        self.row(table::EXEC_OPERATION_COUNT, &operation)
    }

    pub fn exec_in_flight(&self, action: &ObjectHash) -> Bytes {
        self.row(table::EXEC_IN_FLIGHT, &action.as_bytes().as_slice())
    }

    pub fn exec_worker(&self, worker: &str) -> Bytes {
        self.row(table::EXEC_WORKER, &worker)
    }

    pub fn exec_platform(&self, platform: &str) -> Bytes {
        self.row(table::EXEC_PLATFORM, &platform)
    }

    pub fn exec_platforms(&self) -> Bytes {
        self.table(table::EXEC_PLATFORM)
    }

    pub fn exec_wake(&self) -> Bytes {
        self.table(table::EXEC_WAKE)
    }

    pub fn exec_secret(&self, name: &str) -> Bytes {
        self.row(table::EXEC_SECRET, &name)
    }

    pub fn exec_secrets(&self) -> Bytes {
        self.table(table::EXEC_SECRET)
    }

    pub fn exec_secret_command(&self, digest: &ObjectHash) -> Bytes {
        self.row(table::EXEC_SECRET_COMMAND, &digest.as_bytes().as_slice())
    }

    pub fn verify_run(&self, run: &str) -> Bytes {
        self.row(table::VERIFY_RUN, &run)
    }

    /// The prefix of every verification run, newest first.
    pub fn verify_runs(&self) -> Bytes {
        self.table(table::VERIFY_RUN)
    }

    pub fn verify_run_step(&self, run: &str, index: u32) -> Bytes {
        self.row(table::VERIFY_RUN_STEP, &(run, index as i64))
    }

    /// The prefix of a run's steps, in order.
    pub fn verify_run_steps(&self, run: &str) -> Bytes {
        self.row(table::VERIFY_RUN_STEP, &(run,))
    }

    pub fn verify_run_of_version(&self, version: &ObjectHash, run: &str) -> Bytes {
        self.row(
            table::VERIFY_RUN_VERSION,
            &(version.as_bytes().as_slice(), run),
        )
    }

    /// The prefix of the runs of `version`, newest first.
    pub fn verify_runs_of_version(&self, version: &ObjectHash) -> Bytes {
        self.row(table::VERIFY_RUN_VERSION, &(version.as_bytes().as_slice(),))
    }

    pub fn verify_run_count(&self, run: &str) -> Bytes {
        self.row(table::VERIFY_RUN_COUNT, &run)
    }

    /// A run of `target` at `at_millis`; later runs sort first.
    pub fn test_run(&self, target: &str, at_millis: i64, id: &[u8]) -> Bytes {
        self.row(table::TEST_RUN, &(target, i64::MAX - at_millis, id))
    }

    /// The prefix of `target`'s runs, newest first.
    pub fn test_runs(&self, target: &str) -> Bytes {
        self.row(table::TEST_RUN, &(target,))
    }

    pub fn test_stats(&self, target: &str) -> Bytes {
        self.row(table::TEST_STATS, &target)
    }

    /// The prefix of every target's stats.
    pub fn incidents(&self) -> Bytes {
        self.table(table::INCIDENT)
    }

    pub fn incident(&self, culprit: u64) -> Bytes {
        seq_key(&self.incidents(), culprit)
    }

    pub fn long_operation(&self, id: &OperationId) -> Bytes {
        self.row(table::LONG_OPERATION, &id.as_bytes().as_slice())
    }

    pub fn test_quarantine(&self, target: &str) -> Bytes {
        self.row(table::TEST_QUARANTINE, &target)
    }

    pub fn all_test_quarantine(&self) -> Bytes {
        self.table(table::TEST_QUARANTINE)
    }

    pub fn all_test_stats(&self) -> Bytes {
        self.table(table::TEST_STATS)
    }

    pub fn exec_nonhermetic(&self, label: &str) -> Bytes {
        self.row(table::EXEC_NONHERMETIC, &label)
    }

    pub fn exec_nonhermetics(&self) -> Bytes {
        self.table(table::EXEC_NONHERMETIC)
    }

    pub fn exec_secret_commands(&self) -> Bytes {
        self.table(table::EXEC_SECRET_COMMAND)
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
