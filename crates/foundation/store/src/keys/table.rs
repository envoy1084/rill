//! Table names, the first element of every key.

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
/// `(rhd, ns, repo, release)` → be64 seq ++ version hash of a release head
pub const RELEASE_HEAD: &str = "rhd";
/// `(rel, ns, repo, release, seq)` → version hash
pub const RELEASE: &str = "rel";
/// `(relh, ns, repo, release, hash)` → be64 seq
pub const RELEASE_BY_HASH: &str = "relh";
/// `(rland, ns, repo, release, seq)` → Landing
pub const RELEASE_LANDING: &str = "rland";
/// `(rlq, ns, repo, release, lane, requested_at, ticket)` → empty; release
/// requests not yet finished, in the order they are served
pub const RELEASE_PENDING: &str = "rlq";
/// `(rln, ns, repo)` → counter moved by every new release request
pub const RELEASE_WAKE: &str = "rln";
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
/// `(rebacs, ns, repo, subject, object, relation)` → empty; the same
/// tuples by subject
pub const REBAC_BY_SUBJECT: &str = "rebacs";
/// `(rebacv, ns, repo)` → counter bumped by every relation change
pub const REBAC_VERSION: &str = "rebacv";
/// `(quar, ns, repo, version)` → why a version is quarantined
pub const QUARANTINE: &str = "quar";
/// `(misc, ns, repo, name)` → small values (policy, rings, cursors)
pub const MISC: &str = "misc";
/// `(xop, ns, space, operation)` → a remote execution operation
pub const EXEC_OPERATION: &str = "xop";
/// `(xopn, ns, space, operation)` → counter moved by every update of the
/// operation; waiters watch it
pub const EXEC_OPERATION_COUNT: &str = "xopn";
/// `(xif, ns, space, action)` → the operation running the action now
pub const EXEC_IN_FLIGHT: &str = "xif";
/// `(xwk, ns, space, worker)` → what a worker is executing
pub const EXEC_WORKER: &str = "xwk";
/// `(xpl, ns, space, platform)` → a platform actions were queued for
pub const EXEC_PLATFORM: &str = "xpl";
/// `(xwake, ns, space)` → counter moved by every queued action; idle
/// workers watch it
pub const EXEC_WAKE: &str = "xwake";
/// `(xsec, ns, space, name)` → an org secret, its value sealed
pub const EXEC_SECRET: &str = "xsec";
/// `(trun, ns, repo, target, newest first, id)` → one run of a test or
/// verification step
pub const TEST_RUN: &str = "trun";
/// `(vrun, ns, repo, run)` → a verification run's header; run ids sort
/// newest first
pub const VERIFY_RUN: &str = "vrun";
/// `(vrstep, ns, repo, run, index)` → one step of a verification run
pub const VERIFY_RUN_STEP: &str = "vrstep";
/// `(vrunv, ns, repo, version, run)` → the runs that verified a version
pub const VERIFY_RUN_VERSION: &str = "vrunv";
/// `(vrunc, ns, repo, run)` → counter moved by every update of the run
pub const VERIFY_RUN_COUNT: &str = "vrunc";
/// `(tstat, ns, repo, target)` → a test's decayed run, failure and flip
/// counts
pub const TEST_STATS: &str = "tstat";
/// `(tq, ns, repo, target)` → Quarantine: a test or step whose failures
/// neither block landing nor trigger reverts
pub const TEST_QUARANTINE: &str = "tq";
/// `(inc, ns, repo, culprit seq)` → Incident: a trunk break and its culprit
pub const INCIDENT: &str = "inc";
/// `(xnh, ns, space, label)` → an action label whose re-run gave other
/// outputs: its results are not cached for a while
pub const EXEC_NONHERMETIC: &str = "xnh";
/// `(xscmd, ns, space, digest)` → a command with secret values, sealed,
/// readable only by workers while its action runs
pub const EXEC_SECRET_COMMAND: &str = "xscmd";
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
/// `(audq, org, stamp)` → AuditRecord waiting to be chained
pub const AUDIT_PENDING: &str = "audq";
/// `(audqn)` → counter of records queued, for the chainer to wait on
pub const AUDIT_PENDING_COUNT: &str = "audqn";
/// `(aud, org, seq)` → AuditEntry
pub const AUDIT: &str = "aud";
/// `(audh, org)` → AuditHead
pub const AUDIT_HEAD: &str = "audh";
/// `(audc, source)` → the last versionstamp projected from a log
pub const AUDIT_CURSOR: &str = "audc";
/// `(audr, org, last_seq)` → AuditRoot
pub const AUDIT_ROOT: &str = "audr";
/// `(auds, org)` → the last sealed sequence, big-endian u64
pub const AUDIT_SEALED: &str = "auds";
/// `(red, org, id)` → Redaction
pub const REDACTION: &str = "red";
/// `(redh, org, hash)` → id of the applied redaction of an object
pub const REDACTED: &str = "redh";
/// `(dk, org, id)` → DataKeyRecord
pub const DATA_KEY: &str = "dk";
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
/// `(idt, org, name)` → TeamRecord
pub const IDENT_TEAM: &str = "idt";
/// `(idsu, org, scim id)` → user name; `(idsg, org, scim id)` → team name
pub const IDENT_SCIM_USER: &str = "idsu";
pub const IDENT_SCIM_GROUP: &str = "idsg";
/// `(idsc, id)` → ScimCredential
pub const IDENT_SCIM_CREDENTIAL: &str = "idsc";
