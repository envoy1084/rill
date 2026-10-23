# Glossary

Short definitions of the terms used across these pages. Each entry links to
the page that owns the concept. Code identifiers are the names used in the
source.

```mermaid
flowchart LR
  WS[Workspace] -->|snapshot / new change| CH[ChangeId]
  CH -->|one or more| CV[ChangeVersion]
  CV -->|FileChange per path| MG[Merge terms if conflicted]
  CH -->|ReviewState: labels, votes, threads| RV[Review]
  RV -->|RequestLand| LD[Lander]
  LD -->|CAS| TR[trunk@TrunkSeq]
  TR -->|Landed event| PL[Post-land verification]
```

## Data

- **ObjectHash**: 32-byte BLAKE3 hash of an object's canonical bytes, keyed
  together with an `ObjectKind` (`blob`, `chunk`, `manifest`, `tree`, `cv`,
  derived). `TreeHash` and `ChangeVersionHash` are aliases. See
  [objects and ids](../02-data/objects-and-ids.md).
- **ChangeId**: random 16-byte id (`ch_...`) that names a change across all
  of its versions. Review state is keyed by it. See
  [objects and ids](../02-data/objects-and-ids.md).
- **ChangeVersion**: one immutable version of a change: parents,
  predecessor, `file_changes`, `subtree_changes`, author, provenance, intent
  and message. Its hash is the version identity. It does not store a root
  tree. See [objects and ids](../02-data/objects-and-ids.md).
- **FileChange**: what happened to one path relative to the first parent:
  `Set` (content, kind, optional copy source), `Delete`, or `Conflict` with
  `Merge` terms. See [objects and ids](../02-data/objects-and-ids.md).
- **ContentRef**: file content as `Inline` bytes, a single `Blob`, or a
  `Chunked` blob manifest. See [content and chunking](../02-data/content-and-chunking.md).
- **`Merge<T>`**: the conflict algebra `A + (C - B) + ...` with adds and
  removes. A resolved value has one add and no removes. Closed under further
  merging and rebasing. See [merge algebra](../02-data/merge-algebra.md).
- **Conflict**: a path whose `FileChange` is `Conflict`. It is data inside a
  version, never an error, and can be reviewed, handed off or resolved later.
  See [merge](../03-core-algorithms/merge.md).
- **Provenance**: who and what produced a version (tool, model, session,
  prompt ref, sponsor). Part of the hashed version. See
  [signing](../05-policy/signing.md).
- **Derived data**: values computed from a version and its parents
  (`root`, `gen`, `phist`, `summary`, `blame@1`), stored by `kind@version`
  and never invalidated. Managed by `DeriveManager`. See
  [derived data](../02-data/derived-data.md).
- **Root tree**: the full tree of a version, derived by applying its changes
  to the parent's root tree. See [trees](../03-core-algorithms/trees.md).

## Storage

- **ObjectStore**: trait for immutable, hash-verified objects. Backends:
  memory, local files, S3. See [storage and keyspace](../02-data/storage-and-keyspace.md).
- **MetaStore**: trait for small mutable keys with transactions
  (conditions, writes, logs, counters). Backends: memory, redb,
  FoundationDB, all on one keyspace layer. See
  [metadata engines](../02-data/metadata-engines.md).
- **Versionstamp**: a 12-byte commit-order position assigned by the
  metadata store; the cursor of stamped logs such as the version log.
- **Event**: what changed in a repository, by whom and when; written by the
  transaction that makes the change, one per change, into the repository's
  event log. See [events and workers](../07-runtime/events-and-workers.md).
- **Job**: durable follow-up work in a queue (`derive`, `index`), enqueued
  by the transaction that needs it and run by a worker holding a leased
  claim. See [events and workers](../07-runtime/events-and-workers.md).
- **Lease**: a renewed, expiring claim on a role (`lander`, `postland`) with
  an epoch that fenced writes check, so a replaced holder cannot write.
- **RepoStore**: typed view of one repository's keyspace over both stores:
  refs, trunk, change info, review, workspace and derived records. See
  [storage and keyspace](../02-data/storage-and-keyspace.md).
- **RepoContext**: the server's cached per-repository service graph (store,
  derive manager, graph, lander, review, search, authorizer, land-ticket bus). See
  [server](../07-runtime/server.md).

## Workflow

- **Workspace**: a principal's server-side in-progress state: base version,
  working `ChangeId`, latest snapshot, sparse profile and an operation log
  for undo. See [workspaces](../04-workflow/workspaces.md).
- **Checkout**: a local working copy of part of a repository, served by
  the client daemon as a mount. See [checkouts](../04-workflow/checkouts.md).
- **Mount**: a working copy the client daemon serves: a base, an overlay
  of local edits, and the files (on disk, or a virtual filesystem). See
  [client daemon](../08-clients/daemon.md).
- **Trunk**: the single landed line; the `trunk` ref plus one row per
  landed version. See [landing](../04-workflow/landing.md).
- **Sequence number** (`TrunkSeq`): gap-free position on trunk, written as
  `trunk@N`. Assigned by `RepoStore::land_on_trunk`. See
  [landing](../04-workflow/landing.md).
- **Stack**: an ordered chain of dependent changes that land as one ordered
  group (all or nothing, or the longest landable prefix); what stays above a
  landing is restacked onto it. See
  [changes and stacks](../04-workflow/changes-and-stacks.md).
- **Release** (`release/<name>`): a line of landings cut from a trunk
  position, moved only by its own lander. See
  [landing](../04-workflow/landing.md#release-lines).
- **Merge driver**: how `.rill/merge.toml` says a conflicted file merges at
  landing (`union`, `generated`, `regenerate`); a rung of the conflict
  ladder. See [landing](../04-workflow/landing.md#conflict-ladder).
- **Intent**: a record (`in_...`) of what work is for: kind, status, owner, optional
  risk override, linked changes. A version may carry an intent id. See
  [intents](../04-workflow/intents.md).
- **Label / vote**: review is votes on named labels (for example `Review`,
  `Verified`) with allowed ranges, per principal. See
  [review](../04-workflow/review.md).
- **Thread**: a comment thread anchored to a change; unresolved threads
  are a submit-policy fact. See [review](../04-workflow/review.md).
- **Interdiff**: the diff between two versions of the same change. See
  [review](../04-workflow/review.md).
- **Lander**: the per-repository pipeline that takes queued land requests,
  rebases onto trunk, verifies, and CASes trunk. See
  [landing](../04-workflow/landing.md).
- **Post-land verification**: `PostLandVerifier` follows trunk in batches,
  verifies landed versions, bisects failures and can revert. See
  [post-land verification](../04-workflow/post-land-verification.md).

## Policy

- **Principal**: an identity of kind `human`, `agent` or `service`
  (`pr_...`). See [identity and auth](../05-policy/identity-and-auth.md).
- **Capability token**: a Biscuit token carrying repo, verbs (`read`,
  `write`, `land`, `review`, `admin`), path prefixes and expiry. It can be
  attenuated offline, only narrower. See
  [capability tokens](../05-policy/capability-tokens.md).
- **Relation / ReBAC**: tuples granting `owner`, `writer`, `reader`,
  `reviewer` or `contributor` on `repo` or `dir:/path` objects, inherited
  down the tree. See [permissions](../05-policy/permissions.md).
- **Risk class**: `trivial`, `low`, `normal`, `sensitive` or `critical`,
  computed from a change.s facts (paths, size, author, intent kind) and
  overridable by an owner. It drives submit policy and
  verification mode. See [submit policy](../05-policy/submit-policy.md).
- **Submit policy**: per-repository Cedar rules evaluated over facts about
  a change to decide whether it may land. See
  [submit policy](../05-policy/submit-policy.md).

## Limitations

- Several terms have no single type: "change" is a `ChangeId` plus its
  `ChangeInfo` and `ReviewState`, and "stack" exists only as review
  dependencies and SDK helpers.

## Code

- [Ids](../../crates/foundation/types/src/ids.rs)
- [Hashes and object kinds](../../crates/foundation/types/src/hash.rs)
- [ChangeVersion and FileChange](../../crates/foundation/types/src/objects/change.rs)
- [Merge](../../crates/foundation/types/src/merge.rs)
- [Review records](../../crates/foundation/types/src/objects/review.rs)
- [Intent and RiskClass](../../crates/foundation/types/src/objects/intent.rs)
- [Workspace](../../crates/foundation/types/src/objects/workspace.rs)
- [Relations](../../crates/domain/policy/src/rebac/model.rs)
- [Capability verbs](../../crates/domain/policy/src/tokens/capability.rs)
- [Mount](../../crates/clients/daemon/src/mount/mount.rs)

---
← Previous: [Crates and layering](crates-and-layering.md) · [Index](../index.md) · Next: [Objects and ids](../02-data/objects-and-ids.md) →
