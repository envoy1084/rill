# Events and workers

Every committed change to a repository's state writes exactly one event, in
the transaction that makes the change. The event log is durable and
ordered, and it is what watch streams read, so a watcher sees every change
once, whichever server made it. Work that a change makes necessary
(deriving and indexing a landing) is a durable job enqueued in the same
transaction and run by workers that claim it under a lease. Roles that must
have one holder at a time (the lander, the post-land verifier) hold a
renewed, fenced lease. Nothing a client can observe lives only in one
process's memory, so any number of servers can share one store.

## Event log

Events are rows `(evt, ns, repo, versionstamp)` holding an `Event {kind,
actor, at}` (`rill-types/src/objects/event.rs`). `RepoStore::event` returns
the two mutations every writer adds to its transaction: the stamped row and
an atomic add to the counter `(evtn, ns, repo)`. Because the event commits
with the change, a failed or retried write records nothing extra, and a
write that commits records its event even if its acknowledgement is lost.

| Event kind | Written by |
| --- | --- |
| `RepoCreated` | `Stores::create_repo_with` |
| `RefMoved` | `RepoStore::update_ref` (`MoveRef`) |
| `VersionStored` | `put_change_version`, and the commit transaction |
| `ReviewSubmitted` | `create_review` |
| `Voted`, `Commented`, `ThreadResolved`, `StackSet`, `StatusChanged` | `review_writes`, from the difference between the review read and the one written; used by every review write, the commit transaction and landing |
| `Landed` | the land transaction, one per trunk position |
| `WorkspaceChanged`, `WorkspaceDeleted` | `operation_tx`, `delete_workspace` |
| `IntentChanged` | `put_intent` |
| `RelationChanged` | `add_relation`, `remove_relation` (`Authorizer::grant` / `revoke`) |
| `PolicyChanged` | `SetPolicy`, through `set_misc_with_event` |

Each writer takes the principal that caused the change; the review service
uses `Principal::service("review")` for changes it makes on its own
(re-evaluation, stacks, new versions), and landings use the lander's.

`rill-events` reads the log:

* `tail(repo, start, filter)` is a stream of `(cursor, Event)`. `Start` is
  `Now` (after the newest event), `Beginning` or `After(cursor)`. Before
  each page (256 events) it reads the event counter. When a page is empty
  it waits for the counter to move past that reading
  (`MetaStore::wait_counter`, at most 5 s per wait), so an event committed
  between the read and the wait still wakes it. Versionstamps follow commit
  order, so the tail never skips an event that commits later with a lower
  cursor.
* `EventFilter {kinds, change, ref_prefix}` is a conjunction; empty
  matches all.
* `run_retention` deletes events older than `RETENTION` (30 days) every
  hour, 1,000 per transaction. The counter is not decremented.

## Watch streams

Each watch spawns a task feeding an `mpsc` channel (256) into the gRPC
stream. It stops on the repository's shutdown token or when the client goes
away.

| RPC | Reads | Start |
| --- | --- | --- |
| `WatchEvents` | the event log through `tail`, with the request's kinds, change and ref prefix | `after`: absent is now, empty is the beginning, otherwise a cursor from an earlier `EventEnvelope` |
| `WatchRefs` | `RefMoved` and `Landed` events; a landing is reported as a `trunk` move with the previous trunk entry as `old` | now |
| `WatchTrunk`, `WatchLog` | the trunk and version log from a cursor (`trunk_range`, `log_after` in pages of 512), re-read whenever the event counter moves | `after_seq` / `after` as before |
| `WatchReview` | the log, filtered to one change, including its land requests' `LandRequested` and `LandStatusChanged` events | now |

## Jobs

`rill-jobs` keeps queues in the metadata store, per repository:

| Rows | Holds |
| --- | --- |
| `(jobq, …, queue, shard, priority, run_at, stamp)` | a job ready from `run_at` |
| `(jobl, …, queue, job)` | a claimed job with its holder and lease expiry |
| `(jobt, …, queue, job)` | the claim's token |
| `(jobd, …, queue, job)` | a dead job and its last error |
| `(jobn, …, queue, metric)` | counters: `enqueued`, `completed`, `retried`, `dead`, `wake` |

* **Enqueue** (`Queue::enqueue`) returns mutations for the caller's
  transaction: a stamped ready row in the shard picked by the job's key
  (FNV-1a, 4 shards) and counter adds. A job exists exactly when the change
  that needed it committed.
* **Claim** scans each priority (high, normal, low) across shards, starting
  at a random shard, for rows with `run_at <= now`. It moves one to `jobl`
  and writes a fresh random token in one transaction conditioned on the
  ready row; a conflict means someone else took it.
* **Renew** rewrites the lease expiry, conditioned on the token.
  **Complete** deletes the claim under the same condition
  (`complete_tx` lets a handler commit its own writes with it). A handler's
  own writes can be fenced with `Claim::fence()`. The token does not change
  on renewal, so a fenced write fails once the job was taken back, however
  often the claim was renewed.
* **Fail** and **reap** turn an attempt into a retry, ready again after
  `base_delay · 2^(attempt-1)` (1 s up to 5 min) scaled by a random
  factor in [0.5, 1] so retries spread out, or, after `max_attempts`
  (5) or a fatal failure, a dead job. `reap` does this for every claim past
  its lease (30 s). `retry_dead` revives a dead job with fresh attempts.
* `Worker` runs a `Handler` over a queue: N claim loops that wait on the
  `wake` counter when idle (at most 1 s, since backoffs do not move it),
  renew each claim every third of the lease while the handler runs and drop
  the attempt if the claim is lost, and a reaper every half lease. Jobs can
  run more than once, so handlers are idempotent or fenced.

The land transaction carries the jobs of a landing (`LandBatch::also`,
built by `rill_lander::land::followups::landing_jobs`): one
`derive_background` job per landed version on the `derive` queue and one
low-priority `index_version` job for the new head on the `index` queue.

## Leases

`(lease, …, role)` holds `{holder, epoch, expires_at}` and `(lepoch, …,
role)` the epoch alone. `try_acquire` takes a free or expired lease and
bumps the epoch; `renew` and `release` are conditioned on the record the
holder has. `LEASE_TTL_MS` is 10 s. `LeaseHolder::current` renews once half
of it has passed and reports `None` while another process holds the role.
`Lease::fence` is a condition on the epoch row: the land transaction and the
post-land cursor write carry it, so a holder that was replaced (a paused or
partitioned process) cannot write even if it still believes it holds the
lease. A standby lander or verifier does nothing until the lease is free.

## Workers

`workers::spawn_repo_workers` runs once per repository context. The
follower runs in every process; the rest only with the `workers` role
([configuration](configuration.md)). `workers::discover_repos` builds the
context of every repository every 5 s, including ones other servers
created, so the remaining servers take over all background work when one
dies.

| Worker | Trigger | After a restart |
| --- | --- | --- |
| Follower, `workers/follower.rs` | `PolicyChanged` and `RelationChanged` in the log | reloads the review policy, drops the cached relation snapshot |
| Lander loop, `Lander::run_forever` | a new request (wake counter) or every `batch_window`, while holding the `lander` lease | a lander that takes the lease serves every stored request |
| Post-land verifier | every `postland.interval_secs`, while holding the `postland` lease | resumes from its fenced cursor |
| Derive worker, `workers/deriver.rs` | the `derive` queue, 2 at a time | unfinished jobs are reaped and rerun |
| Index worker, `workers/indexer.rs` | the `index` queue, while holding the `search` lease (`workers/leader.rs`); queues the trunk head once at start, for trunk moved without a landing (imports) | the next holder reloads the index and reruns unfinished jobs |
| Event retention | hourly | stateless |

One server-wide task, `workers::sweep_idempotency`, deletes expired
idempotency records every 10 minutes.

The derive handler calls `DeriveManager::ensure_background`; the index
handler calls `index_version` and flushes. Both treat errors as retryable
and bad payloads as fatal. `ExecVerifier` and `ReviewHooksAdapter`
(`workers/verifier.rs`, `workers/hooks.rs`) are adapters wired in at build
time, not tasks.

## Limitations

* Workers are tasks in a server process (`--roles workers` makes a
  process that only runs them); there are no separate fleets per queue.
* Waiting for events or jobs polls the counter (every 20 ms) on every
  engine; the FoundationDB engine does not use watches yet.
* A cursor older than the retention window resumes at the oldest retained
  event without saying events were dropped.
* A job's claim scan reads each shard and priority separately; with many
  shards and empty queues an idle claim costs `shards × 3` range reads.

## Code

* [rill-events](../../crates/rill-events/src/lib.rs), [rill-jobs](../../crates/rill-jobs/src/lib.rs)
* [rill-store repo/events.rs](../../crates/rill-store/src/repo/events.rs), [repo/leases.rs](../../crates/rill-store/src/repo/leases.rs), [repo/reviews.rs](../../crates/rill-store/src/repo/reviews.rs)
* [rill-lander land/followups.rs](../../crates/rill-lander/src/land/followups.rs)
* [workers/mod.rs](../../crates/rill-api/src/workers/mod.rs), [workers/deriver.rs](../../crates/rill-api/src/workers/deriver.rs), [workers/indexer.rs](../../crates/rill-api/src/workers/indexer.rs), [workers/leader.rs](../../crates/rill-api/src/workers/leader.rs), [workers/follower.rs](../../crates/rill-api/src/workers/follower.rs)
* [services/repo/watch/](../../crates/rill-api/src/services/repo/watch/mod.rs), [services/review/watch.rs](../../crates/rill-api/src/services/review/watch.rs)

---
← Previous: [API surface](api-surface.md) · [Index](../index.md) · Next: [Configuration](configuration.md) →
