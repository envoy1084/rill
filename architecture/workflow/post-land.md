# Post-land verification

Verification is continuous: trunk advances optimistically and a worker,
`PostLandVerifier`, follows it afterwards, verifying batches of positions,
bisecting a red batch to the first red position and, when policy allows,
landing a revert of it. The invariant: every trunk position at or below
the worker's cursor has a `VerificationRecord`, either verified directly or
inferred from the batch it belongs to, and a red record names the culprit
position it inherits from.

```mermaid
flowchart LR
    T[(trunk/<seq>)] --> W[PostLandVerifier.run_once<br/>every interval_secs]
    W -->|cursor..to| V[verify_seq(to)]
    V -->|green| G[fill Green, inferred]
    V -->|red, base already red| R1[fill Red, same culprit]
    V -->|red, base green| BI[bisect → culprit]
    BI --> G2[fill Green before, Red after]
    BI -->|auto_revert| AR[schedule_auto_revert<br/>sleep revert_grace]
    AR -->|no forward fix| RV[revert → land_now]
    RV --> L[Lander]
    W --> REC[(postland@1 records,<br/>postland_index ring,<br/>postland_cursor)]
```

## Worker

`PostLandVerifier` ([worker.rs](../../crates/rill-lander/src/postland/worker.rs))
is built by `RepoRegistry::build` when `[postland].enabled` and spawned by
`spawn_repo_workers` with `run_forever(interval, shutdown)`; it is also
reachable through the `Bisect` RPC. `run_once` takes `sweep_lock` (a
`tokio::Mutex`; a second caller waits rather than racing the cursor) and:

1. `cursor = load_cursor()` (`TrunkSeq::ZERO` when never stored); return
   `None` when `head <= cursor`.
2. `to = min(head, cursor + batch_size)`.
3. `base_culprit`: when the record at `cursor` is `Red`, its `culprit`
   (or `cursor` itself).
4. `verify_seq(to)`: materialize the root of `trunk_at(to)`, call the
   lander's `Verifier`, write a direct record and publish
   `LandEvent::Verification { seq, green }`. A direct result that disagrees
   with an earlier direct record marks the position `Flaky`.
5. Tip green ⇒ `fill(cursor, to, Green)`. Tip red with a red base ⇒
   `fill(cursor, to, Red, culprit = base_culprit)` ("still red"). Tip red
   on a green base ⇒ `bisect(cursor, to)`, then `fill(cursor, found,
   Green)` and `fill(found, to, Red, culprit = found)`.
6. `store_cursor(to)`.

`fill` writes an `inferred: true` record for every position in `(from,
to)` that has no record yet and publishes a `Verification` event for each.
`bisect(green, red)` is a binary search calling `verify_seq` on midpoints
(each verification is a direct record; the verifier's action cache makes
repeats cheap), marks the culprit's own record with `culprit = itself`,
publishes `LandEvent::Culprit` and, when `LandPolicy.auto_revert`, calls
`schedule_auto_revert`.

The verifier is the same `Verifier` the lander uses for pre-land checks:
`ExecVerifier` over `rill-exec` when `[verify].enabled`, else
`AlwaysGreen`. With `AlwaysGreen` every record is green and bisection
never runs.

### Batching

`batch_size` (`[postland].batch_size`, default `DEFAULT_BATCH_SIZE = 8`)
bounds how far one sweep advances. With one landing per sweep the worker
verifies every position directly; when it falls behind it verifies only
the batch tip and infers the rest, paying for bisection only when the tip
is red on a green base.

## Records

`VerificationRecord` ([record.rs](../../crates/rill-lander/src/postland/record.rs)):

| Field | Meaning |
| --- | --- |
| `seq`, `version` | Trunk position and the version there. |
| `status` | `Green`, `Red`, or `Flaky` (two direct verifications disagreed). |
| `details` | Verifier text, or `inferred: <why> (from..to)`. |
| `culprit` | First red position responsible; a culprit's own `seq` on itself; `None` on green records. |
| `inferred` | True when filled from a batch rather than verified. |
| `at` | Timestamp. |

| Key | Value |
| --- | --- |
| derived `postland@1/<version>` | the record (`record_of`; `record_at(seq)` goes through `trunk_at`) |
| misc `postland_index` | ring of the last `POSTLAND_RING = 1000` records, one per `seq`, sorted on read |
| misc `postland_cursor` | `u64` big-endian, last position verified through |
| misc `verify_reports:<seq>` | JSON `Vec<Report>` filed by external runners through `ReportVerification` ([lander.rs](../../crates/rill-server/src/services/lander.rs)); one per `(target, reporter)`, a re-run replaces |

`GetVerification`, `ListLandings` and `GetTrunkStatus` combine the
post-land record (as target `postland`, reporter `svc:lander`) with the
external reports and aggregate worst-wins: any `Failed`/`Inherited` wins
over `Running` over `Pending` over `Skipped` over `Passed`; no reports
means `Pending`. An inferred red record whose culprit is another position
maps to `Inherited`; a direct red or flaky record maps to `Failed`.
`last_verified_seq` in trunk status is the highest green record in the
ring.

## Culprit finding, revert and grace

`build_revert(seq, reason, actor, tool)` ([revert.rs](../../crates/rill-lander/src/revert.rs))
creates a new change (fresh `ChangeId`) whose file changes invert the tree
diff from the landed version's first parent to itself: each path gets the
parent's content back, or `Delete` when the path did not exist. The
revert is parented on the landed version so that landing it rebases it
onto the current head with a real three-way merge; later edits to the same
files surface as a rebase conflict rather than being overwritten.
Provenance carries `tool` (`lander:revert` or `lander:auto-revert`) and
`extra["reverts"] = seq`, `extra["reverted_version"]`; `intent` is copied
from the reverted version; the message is `Revert trunk@N: <summary>`.

`revert` lands it through `land_now` with `risk = Normal` and
`verify_override = Some(VerifyMode::None)` (no pre-land verification).
Anything but `Landed` is `RevertFailed { seq, reason }` → gRPC `Aborted`.
On success it publishes `LandEvent::Reverted { culprit, revert }`.

`schedule_auto_revert` spawns a task that sleeps `revert_grace` and then
calls `auto_revert`, which skips the revert when `has_forward_fix(seq)`:
some version landed at or after `seq` whose provenance `reverts == seq`,
whose message contains the text `trunk@<seq>`, or whose intent title or
body contains it. Otherwise it reverts as the service principal
`svc:lander`. The task holds only an `Arc<Lander>`; a server restart
during the grace period forgets the pending revert, and nothing persists
that a culprit is awaiting one.

`RevertSeq` (`rill revert <seq> -m`) calls `revert` as the caller with the
`land()` permit. `Bisect` (`rill bisect <from> <to>`) requires the
worker to be enabled (`FailedPrecondition` otherwise), runs
`postland.bisect(from, to)` without checking that `from` is green or `to`
red, and returns the culprit plus every position in `(from, to]` that has
a record. The `--target` flag is accepted by the CLI and SDK but the RPC
does not filter by target.

```mermaid
sequenceDiagram
    participant W as PostLandVerifier
    participant V as Verifier
    participant S as RepoStore
    participant L as Lander
    W->>S: load_cursor → 8; trunk_head → 12
    W->>V: verify(root of trunk@12) → red
    W->>S: put_record(12, Red, direct)
    Note over W: record at 8 is Green → bisect(8, 12)
    W->>V: verify(trunk@10) → green
    W->>V: verify(trunk@11) → red
    W->>S: set_culprit(11, 11); fill 9..10 Green; fill 12 Red culprit 11
    W-->>W: publish Culprit{11}
    W->>S: store_cursor(12)
    Note over W,L: auto_revert: sleep revert_grace
    W->>L: has_forward_fix(11)? no → build_revert(11) → land_now(verify None)
    L-->>W: Landed{13}; publish Reverted{11, 13}
```

## `ReviewHooks` in the server

`rill-lander` never depends on `rill-review`. The server adapts the two
with `ReviewHooksAdapter` ([hooks.rs](../../crates/rill-server/src/workers/hooks.rs)),
which holds the `ArcSwap<ReviewService>` handle so a policy reload is seen
by in-flight landings:

| Hook | Review call | `ReviewNotFound` | Other errors |
| --- | --- | --- | --- |
| `mark_landing(change)` | `mark_landing` (`Landable → Landing`) | Ok: reverts and dev-mode direct lands have no review. | `Hook` error; the lander logs it and continues. |
| `mark_landed(change, seq, version)` | `mark_landed(change, seq)` (`Landing`/`Landable → Landed`, `landed_seq`) | Ok | logged by the lander |
| `return_to_owner(change, reason)` | `landing_failed` (`Landing → Open` then `refresh`); on `InvalidStatus` just `refresh`; then posts a change-level thread from `svc:lander` addressed `to` the owner: "Landing was returned to the owner: … Rebase the change onto trunk, resolve any conflicts and request landing again." | Ok | `Hook` error, logged |
| `dependents_of(change)` | `stack_of`, members after `change` | empty | `Hook` error, logged |

`mark_landing` failing (for example the change was abandoned after
`RequestLand`) does not stop the landing: the lander logs the hook error
and proceeds to rebase and CAS. The review-side guard is therefore
advisory for the lander and authoritative only for the review record.

## Configuration

| Key | Default | Effect |
| --- | --- | --- |
| `[postland].enabled` (`RILL_POSTLAND_ENABLED`) | `false` | build and run the worker; enables `Bisect` |
| `[postland].interval_secs` (`RILL_POSTLAND_INTERVAL_SECS`) | 30 | sleep between sweeps (minimum 1) |
| `[postland].batch_size` | 8 | positions per sweep |
| `[lander].auto_revert` | `false` | revert bisected culprits |
| `[lander].revert_grace` | 15 min | wait for a forward fix first |
| `[verify].enabled` (`RILL_VERIFY_ENABLED`) | `false` | use `ExecVerifier` instead of `AlwaysGreen` |

## Limitations

* The cursor only moves forward; a position below the cursor is never
  re-verified except by an explicit `Bisect` or a bisection that lands on
  it.
* `Flaky` is terminal for scoring: it maps to `Failed` on the wire and
  counts as not green for `is_green`, so a flaky batch tip triggers
  bisection.
* Pending auto-reverts live in a spawned task and are lost on restart.
* `has_forward_fix` is a substring match on `trunk@N` in messages and
  intents.
* `ReportVerification` stores external reports as JSON in `misc` without
  size limits or retention.
* A revert that conflicts on rebase fails with `RevertFailed`; nothing
  retries it or notifies anyone beyond the log line and the lander's
  `Conflict` event.

## Code

* [rill-lander/src/postland/worker.rs](../../crates/rill-lander/src/postland/worker.rs), [record.rs](../../crates/rill-lander/src/postland/record.rs)
* [rill-lander/src/revert.rs](../../crates/rill-lander/src/revert.rs)
* [rill-lander/src/traits.rs](../../crates/rill-lander/src/traits.rs)
* [rill-server/src/workers/hooks.rs](../../crates/rill-server/src/workers/hooks.rs), [verifier.rs](../../crates/rill-server/src/workers/verifier.rs), [mod.rs](../../crates/rill-server/src/workers/mod.rs)
* [rill-server/src/services/lander.rs](../../crates/rill-server/src/services/lander.rs)
* [rill-server/src/config.rs](../../crates/rill-server/src/config.rs)
* [rill-cli/src/commands/land.rs](../../crates/rill-cli/src/commands/land.rs) (`verify`, `revert`, `bisect`, `landings`)

## See

* [landing](landing.md) for the pipeline a revert goes through.
* [review](review.md) for the status transitions the hooks drive.
* [changes](changes.md) for how a revert is a new change.
