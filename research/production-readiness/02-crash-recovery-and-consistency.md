# Crash recovery and consistency

Rill's writes span two stores and several keys that are not updated in one
transaction: an object `put`, then a metadata `transact`, then follow-up
records and hooks. A process that dies between any two of those steps
leaves state that no code today detects or repairs. The invariant that
matters most: the trunk sequence (`ref/trunk`, `trunk/<seq>`,
`trunkh/<hash>`) is written atomically and is the only authoritative
record of what landed; everything else (landing records, review status,
search index, post-land cursor, tickets) is derived and must be
reconstructible from it.

## Current state

### Write paths and their atomic units

| Path | Steps in order | Atomic unit | Idempotent on retry |
| --- | --- | --- | --- |
| `RepoStore::put_change_version` ([repo.rs](../../crates/rill-store/src/repo.rs)) | pre-read `ci/`; object `put`; one `transact` (`ci/` set + `log/` append + `chv/` append) | the `transact` | no: pre-read only; concurrent or crash-retried callers double-append (see [01](01-storage-durability.md)) |
| `commit_version` ([write.rs](../../crates/rill-server/src/services/repo/write.rs)) | `put_change_version`; derive `RootTree` (writes `dd/`); `review.add_version`; publish event | none across steps | partially: step 1 as above, review `add_version` is a CAS on `ch/` |
| `land_on_trunk` ([repo.rs](../../crates/rill-store/src/repo.rs)) | one `transact` with `expect ref`, `expect_absent trunk/<seq>`, three `Set`s | yes | yes: the `expect` fails on a retry |
| Lander `finalize` ([pipeline.rs](../../crates/rill-lander/src/land/pipeline.rs)) | `record_landing` (`dd/land@1` + `misc/landings` ring, under `record_lock`); set ticket status; `mark_landed` hook (review `ch/` CAS); publish `Landed`; `dependents_of` | each store write separately | no: a crash after the CAS leaves no landing record and the review stuck in `Landing` |
| `append_operation` ([repo.rs](../../crates/rill-store/src/repo.rs)) | `transact` (`expect_absent opi` + `op/` append + `ws/` set); separate `set opi` | first transaction only | no: crash between the two leaves an operation without its index, so a retried `OperationId` appends a second operation |
| Workspace `new_change` / `update_base` ([workspace.rs](../../crates/rill-server/src/services/workspace.rs)) | `commit_version`; then `append_operation` | none | the version is content-addressed; the op is not |
| `PutBlob` / `PUT /o/` | `store_content` chunks and manifest, each a separate object `put` | per object | yes: content-addressed |
| Git import ([importer.rs](../../crates/rill-git-import/src/import/importer.rs)) | per commit `put_change_version`; `ImportMap::save` every `save_every` commits and on exit; then `land_first_parent_history` CAS per version | per version / per CAS | yes by design: the map is saved on failure, landing skips versions with a `trunkh/` entry and refuses with `TrunkDiverged` if trunk moved |
| Search index flush ([search_index.rs](../../crates/rill-search/src/index/search_index.rs)) | dirty shards, then docs table, then blob markers, each a `put_derived` | per key | partially: the order is chosen so a crash leaves markers behind shards; a marker without its shard means a blob is never re-indexed |
| Post-land sweep ([postland/worker.rs](../../crates/rill-lander/src/postland/worker.rs)) | `put_record` per position (`dd/postland@1` + `misc/postland_index` ring); `store_cursor` last | per key | yes: records are keyed by version; a crash before `store_cursor` re-verifies the batch |
| `SetPolicy` ([admin.rs](../../crates/rill-server/src/services/admin.rs)) | `set_misc`; `reload_policy` in process | one key | yes |
| ReBAC `grant`/`revoke` | one key `set`/`delete`; local cache invalidate | one key | yes |

### In-process state that does not survive a crash

* The land queue ([queue.rs](../../crates/rill-lander/src/queue.rs)):
  `pending` is a `VecDeque` in memory; tickets are `u64`s from an
  `AtomicU64` starting at 1 in every process. `RequestLand` returns a
  ticket, and a restart forgets every queued request. `ticket_status`
  ([common.rs](../../crates/rill-server/src/services/common.rs)) falls back
  to the review state, which still says `Landing` for a change whose
  request was lost.
* `RepoContext::tickets` (`DashMap`): ticket-to-change mapping, lost on
  restart; after a restart ticket ids restart at 1 and can collide with
  ids clients still hold.
* `Lander::record_lock` and `PostLandVerifier::sweep_lock`: `tokio::Mutex`
  in the process. Two processes against one store race on the
  `misc/landings` ring (read-modify-write) and on the post-land cursor.
* `Runner::in_flight` ([runner.rs](../../crates/rill-exec/src/runner.rs)):
  per-digest execution lock in memory; two processes execute the same
  action twice and the second `cache.put` overwrites the first.
* The trigram index postings: whole in memory, reloaded from shards on
  `SearchIndex::open`; anything indexed since the last `flush` is lost. The
  indexer flushes after every landed version, so the window is one landing.
* `AuthorizerSnapshot` cache (500 ms TTL): stale for up to the TTL on
  other replicas after a revoke; see [05](05-authorization-audit.md).
* Pending auto-reverts: `schedule_auto_revert`
  ([revert.rs](../../crates/rill-lander/src/revert.rs)) is a
  `tokio::spawn` holding an `Arc<Lander>` that sleeps for `revert_grace`
  (15 min default) and then reverts. Nothing is persisted; a restart
  inside the grace window drops the revert with no log line.
* Review-side guards in the pipeline are advisory: `process`
  ([pipeline.rs](../../crates/rill-lander/src/land/pipeline.rs)) logs a
  `mark_landing` hook error and continues, so a change abandoned between
  `RequestLand` and the sweep still lands.
* Review writes: `submit` writes with `expected = None` after a separate
  existence check, so two concurrent submits of one change both succeed
  and the second overwrites the first; `vote` and `resolve` compute their
  facts once before entering the CAS retry loop, so a retry re-applies
  stale facts. Intent records (`put_intent`, [repo.rs](../../crates/rill-store/src/repo.rs))
  have no CAS at all: concurrent `UpdateIntent` calls lose updates.
* Workspace operation log: `OperationKind::Describe`, `Resolve` and
  `Other` are never recorded by any handler, and `undo`
  ([workspace.rs](../../crates/rill-server/src/services/workspace.rs))
  restores only `base` and `snapshot`, not `owner`, `sparse` or
  `working_change`, so undoing a `Handoff` or `SetSparse` records an
  `Undo` operation that changes nothing.
* The git importer ([land.rs](../../crates/rill-git-import/src/import/land.rs))
  calls `land_on_trunk` directly: imported trunk positions have no
  `dd/land@1` record, publish no `LandEvent`, and are indexed only when
  `grep` hits them on demand or the indexer restarts and sees the head.
* The Biscuit issuing key when no `root_private_key_hex` is configured
  ([authenticator.rs](../../crates/rill-server/src/auth/authenticator.rs)):
  generated per process, so every issued token dies with the process.

### The single-lander assumption

[ARCHITECTURE.md](../../docs/ARCHITECTURE.md) states one lander per repo.
Nothing enforces it: `RepoRegistry::get` spawns workers in every process
that resolves the repository. Two replicas each run `run_forever`, each
drains its own queue, and both call `land_on_trunk`. The CAS guarantees a
single winner per sequence, and the loser's pipeline rebases and retries
(`max_retries = 8`), so trunk stays consistent, but landing records,
`mark_landed` and the ring are updated by whichever process won, with no
cross-process lock. The post-land verifier on both replicas reads and
writes one cursor; interleaved sweeps can mark positions twice and run the
auto-revert twice (`schedule_auto_revert` is a `tokio::spawn` with a sleep;
`has_forward_fix` would see the first revert and skip the second only if
it landed before the second wakes).

### What exists for recovery

Nothing. There is no `rill-server fsck`, no startup reconciliation, no
command to re-derive landing records from `trunk/`, no sweep for `.tmp-*`
files, and no detection of a `ch/` record stuck in `Landing`. The health
endpoints ([admin.rs](../../crates/rill-server/src/services/admin.rs)
`health_components`) probe the stores but not their consistency.

## Why it blocks production

1. **Mid-land crash.** `land_on_trunk` commits, the process dies before
   `finalize`. Trunk has the version; `GetTrunkStatus`'s `recent` list and
   `land_event`'s `landed_by` fall back to the author; the review stays
   `Landing` forever, so `RequestLand` returns `already landing` and the
   change can neither be re-landed nor abandoned through the normal flow.
   The stacked dependents never get `DependentsReady`.
2. **Mid-import crash.** The importer saves the map every `save_every`
   commits; a crash loses up to that many `put_change_version` calls'
   worth of map entries, and the retry re-imports those commits. Because
   `put_change_version` pre-checks `ci/`, a re-import of an already stored
   version is a no-op, so this is tolerable. A crash during
   `land_first_parent_history` is safe (CAS). A crash during `map.save`
   itself (one `set_misc`) is atomic.
3. **Mid-index crash.** Between `put_shard` and `put_marker` nothing is
   inconsistent; between `put_marker` of blob X and the next flush, blob X
   is marked indexed while its postings were never persisted if the shard
   was not dirty at the time. Search silently misses that file forever.
4. **Mid-blob-upload crash.** Chunks are written, the manifest is not;
   nothing references the chunks. Orphaned objects accumulate with no GC
   ([03](03-backup-restore-and-migration.md)). A client retry re-uploads
   and succeeds, so correctness holds.
5. **Queue loss.** A restart during a busy landing window drops every
   `Queued` request. Clients polling `GetLandStatus` get `Returned: ticket
   expired` or, after another process restart assigns the same ticket id
   to a different change, a status for someone else's change.
6. **Double worker.** Running two replicas (for availability, the first
   thing an operator tries) silently violates the one-lander assumption
   with the failure modes above.
7. **Local store temp files.** Thousands of `.tmp-*` files after repeated
   crashes under load; `has()` still reports the object missing (correct),
   but disk fills.

## Requirements

* R1. Every multi-step write path is either one `transact` or is
  reconstructible: a documented repair derives the follow-up records from
  the authoritative key. For landings: `dd/land@1`, the `landings` ring,
  and the review `landed_seq` are derivable from `trunk/` plus `chv/`.
* R2. A `rill-server fsck [--repair]` command exists and is idempotent. It
  reports: trunk entries without landing records; `ch/` records in
  `Landing` whose version has a `trunkh/` entry (repair: `mark_landed`) or
  has no queued request (repair: `landing_failed`); `chv/` rows whose
  version object is missing; duplicate `log/` rows; blob markers without
  shards; `.tmp-*` files older than one hour.
* R3. Startup reconciliation runs the cheap subset of R2 (stuck `Landing`
  reviews, missing landing records for the last N trunk entries) before
  the lander starts accepting requests.
* R4. Land requests are durable: `RequestLand` writes the request to the
  metadata store before returning a ticket; the ticket id is globally
  unique (ULID or `(repo, seq)` from an `AppendLog`); a restarted server
  resumes the queue from the store.
* R5. Exactly one lander, one post-land verifier and one indexer per
  repository run across all replicas, enforced by a lease in the metadata
  store, not by deployment convention.
* R6. A fault-injection suite runs in CI: for each write path, kill the
  process at every await boundary and assert that `fsck` reports nothing
  after `--repair` and that a client retry converges.
* R7. Orphaned temp files are removed at startup and by a periodic sweep.

## Design direction

### Recommended: durable land queue plus a metadata lease

Two changes carry most of the weight: persist the land request, and elect
the per-repo workers through a lease row in the `MetaStore`.

```mermaid
sequenceDiagram
    participant C as Client
    participant S as Server (any replica)
    participant M as MetaStore
    participant L as Lander (lease holder)
    C->>S: RequestLand(change)
    S->>M: transact: expect_absent lq/<repo>/<change>; AppendLog lq/<repo>/ {request}
    M-->>S: seq = ticket
    S-->>C: ticket lq#seq
    loop every batch_window
        L->>M: expect lease/<repo> == me, set expiry
        L->>M: scan lq/<repo>/ (pending)
        L->>L: pipeline: rebase, verify
        L->>M: transact: land_on_trunk + set lq/<repo>/<seq> = Landed + set dd/land@1
        L->>M: review mark_landed (CAS on ch/)
    end
    C->>S: GetLandStatus(lq#seq)
    S->>M: get lq/<repo>/<seq>
```

Details:

1. **Durable queue.** New table `lq/<repo>/<be64 seq>` holding the
   `LandRequest` plus a status enum; `lqi/<repo>/<change-id>` as the
   uniqueness index so one change has at most one open request. The ticket
   becomes `lq#<seq>`; `LandQueue` becomes a cache over the table. Status
   writes go through `transact` with `expect` on the previous status so a
   lost lease holder cannot overwrite a newer status.
2. **Fold `record_landing` into the land transaction.** `land_on_trunk`
   takes an extra list of `Set`s (the `dd/land@1` record and the queue
   status) so the authoritative trunk write and the landing record commit
   together. Drop the `misc/landings` ring in favour of a scan over
   `trunk/` joined with `dd/land@1` (O(limit) reads), which also removes
   the `record_lock`.
3. **Review reconciliation.** `mark_landed` stays a separate CAS, but
   startup and `fsck` repair the `Landing`-with-`trunkh` case. Add a
   `landing_started_at` to `ReviewState` so a stale `Landing` without a
   queue row older than `max_wait` is returned to the owner.
4. **Lease.** `lease/<repo>/<role>` with value `(holder_id, expires_at)`.
   Acquire with `expect_absent` or `expect(old)` when expired; renew every
   `ttl/3`; workers check `holder == me` before every store write that is
   not a CAS and stop on loss. Holder id is a per-process ULID. This is a
   fencing token in the Chubby/ZooKeeper sense, not a distributed lock
   with strong guarantees: a CAS still protects trunk; the lease only
   prevents duplicated side effects.
5. **Fault injection.** A `FailpointMetaStore` and `FailpointObjectStore`
   wrapper (feature `failpoints`) that panics or returns `Backend` after
   the Nth call matching a name; the test harness runs each path in a child
   process (`std::process::Command` on the test binary with an env var),
   kills it, reopens the store, runs `fsck --repair`, and asserts
   invariants. The `fail` crate pattern (as used by TiKV) fits.
6. **Temp sweep.** `LocalObjectStore::open` removes `.tmp-*` files whose
   pid is not alive or whose mtime is older than one hour.

### Alternatives considered

* **Make every path a single transaction by storing objects in the
  `MetaStore`.** Rejected in [01](01-storage-durability.md).
* **Write-ahead intent log for multi-step paths (saga).** A generic
  `intent/` row written before each multi-step operation and cleared after.
  More general than point fixes, but every path still needs a specific
  compensator; the lease plus `fsck` covers the same ground with less
  machinery. Revisit if the number of multi-step paths grows.
* **External coordinator (etcd, ZooKeeper) for the lease.** Adds a
  service; the `MetaStore` already offers conditional writes and FDB gives
  linearizability. Rejected.
* **Idempotency keys for every RPC at the transport layer.** Workspace
  operations already carry `operation_id`; extending that pattern to
  `CreateChangeVersion` is unnecessary because content addressing already
  provides it once the `expect_absent` fix lands.

### Trade-offs

Persisting the queue adds one transaction per land request and one per
status transition. Landing is already seconds to minutes of rebase and
verification; the cost is invisible. The lease introduces a failover delay
of up to one TTL (recommend 10 s) during which no landing happens.

## Work breakdown

1. **S** Fix `put_change_version` and `append_operation` atomicity (shared
   with [01](01-storage-durability.md) steps 1-2).
2. **M** `lq/` table, durable `LandQueue`, ticket format `lq#<seq>`,
   status CAS; migrate `GetLandStatus`, `GetTrunkStatus`, `WatchReview`.
3. **M** Fold `dd/land@1` into the land transaction; replace the
   `landings` ring with a trunk scan; delete `record_lock`.
4. **M** Lease table and `LeaseHolder` helper; gate lander, post-land
   verifier and indexer on it; stop workers on loss.
5. **M** `rill-server fsck` with the checks in R2 and `--repair`; startup
   reconciliation for stuck reviews and missing landing records.
6. **S** Temp-file sweep in `LocalObjectStore::open` plus a periodic task.
7. **L** Failpoint wrappers and the child-process fault-injection suite
   covering: land, import, index flush, blob upload, workspace op, policy
   set, relation grant.
8. **S** `ReviewState::landing_started_at` and the stale-`Landing`
   timeout.
9. **M** Post-land verifier: make `run_once` idempotent across replicas by
   CAS on the cursor (`expect(cursor == from)`), and persist scheduled
   auto-reverts as `misc/pending_revert:<seq>` rows with a due time that
   the lease holder polls, so a restart resumes them.
10. **S** Make `mark_landing` failure abort the landing (return the request
    to its owner) instead of logging; add `expected` to review `submit`
    and recompute facts inside the `vote`/`resolve` retry loop; give
    `put_intent` an `expected` parameter.
11. **S** Record `Landing` records for imported trunk positions (or have
    the importer go through `Lander::land_now` with `VerifyMode::None`),
    and make `undo` restore the full `Workspace` snapshot by storing it in
    the operation.

## Risks and open questions

* The durable queue changes the `LandTicket` wire format (`land#N` to
  `lq#N`); the SDK and CLI parse it in `parse_ticket`. Needs a
  compatibility window.
* Lease TTL versus `LandPolicy::max_wait` (30 min default): a verification
  that runs longer than the lease must keep renewing from inside the
  pipeline, or the lease must be renewed by a separate task that the
  pipeline checks. The latter is simpler.
* `has_forward_fix` matches on message text containing the sequence
  number; two replicas can both decide no forward fix exists in the grace
  window. The lease fixes the duplicate; the heuristic itself is a separate
  product question.
* Fault injection at every await boundary is combinatorial; the plan is to
  inject at store-call granularity (every `transact`, `put`, `set`), which
  is where the atomicity boundaries are.
* The search index "marker without shard" case may already be impossible
  given `docs_dirty` and the flush ordering; the fault-injection suite
  should decide rather than reasoning about it.

## References

Source:

* [crates/rill-store/src/repo.rs](../../crates/rill-store/src/repo.rs)
* [crates/rill-lander/src/land/pipeline.rs](../../crates/rill-lander/src/land/pipeline.rs)
* [crates/rill-lander/src/land/lander.rs](../../crates/rill-lander/src/land/lander.rs)
* [crates/rill-lander/src/land/record.rs](../../crates/rill-lander/src/land/record.rs)
* [crates/rill-lander/src/queue.rs](../../crates/rill-lander/src/queue.rs)
* [crates/rill-lander/src/postland/worker.rs](../../crates/rill-lander/src/postland/worker.rs)
* [crates/rill-lander/src/revert.rs](../../crates/rill-lander/src/revert.rs)
* [crates/rill-server/src/services/repo/write.rs](../../crates/rill-server/src/services/repo/write.rs)
* [crates/rill-server/src/services/workspace.rs](../../crates/rill-server/src/services/workspace.rs)
* [crates/rill-server/src/services/common.rs](../../crates/rill-server/src/services/common.rs)
* [crates/rill-server/src/state/registry.rs](../../crates/rill-server/src/state/registry.rs)
* [crates/rill-server/src/workers/mod.rs](../../crates/rill-server/src/workers/mod.rs)
* [crates/rill-server/src/workers/indexer.rs](../../crates/rill-server/src/workers/indexer.rs)
* [crates/rill-exec/src/runner.rs](../../crates/rill-exec/src/runner.rs)
* [crates/rill-git-import/src/import/importer.rs](../../crates/rill-git-import/src/import/importer.rs)
* [crates/rill-git-import/src/import/land.rs](../../crates/rill-git-import/src/import/land.rs)
* [crates/rill-search/src/index/search_index.rs](../../crates/rill-search/src/index/search_index.rs)
* [crates/rill-store/src/local/objects.rs](../../crates/rill-store/src/local/objects.rs)

Prior art:

* Chubby / ZooKeeper lease and fencing-token patterns (Burrows 2006;
  Kleppmann, "How to do distributed locking").
* Mononoke's `pushrebase` and bookmark CAS with a durable queue of pending
  pushes; Sapling's `hg sl` recovery model.
* Gerrit's `NoteDb` and its `ReindexAfterRefUpdate` and `gerrit reindex`
  for rebuilding derived data from refs.
* Jujutsu operation log (`jj op log`, `jj op restore`): every mutation is
  an operation record, which is the shape the workspace op log already has.
* TiKV `fail` crate failpoints; FoundationDB's simulation testing as the
  gold standard for fault injection.
* `git fsck` and `git gc --prune` as the reference for a repair tool's
  checks and output.
