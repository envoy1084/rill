# Workspaces

A workspace is a server-side record of one principal's in-progress state on
a repository: a base version, a working change id, an optional snapshot
tree and a sparse profile. Every mutation is an `Operation` appended to the
workspace's log. Each operation records the resulting `base` and
`snapshot`, so undo moves a pointer back and never computes an inverse.

Client checkouts in `.rill/` are a separate mechanism that may be attached
to a workspace; see [checkouts](checkouts.md).

```mermaid
flowchart LR
    C[Create] --> S[Snapshot tree]
    S --> N[NewChange]
    N -->|base := new version<br/>working_change := fresh id| S
    N --> U[UpdateBase]
    U -->|rebase working change| S
    X[Undo] -.->|base, snapshot :=<br/>parent op's resulting_*| S
```

## Record

`Workspace` ([workspace.rs](../../crates/rill-types/src/objects/workspace.rs)):

| Field | Meaning |
| --- | --- |
| `id`, `repo`, `name` | Identity. `WorkspaceId` prints as `ws_<hex>`. |
| `owner` | The creator, until `Handoff` changes it. |
| `base` | `ChangeVersionHash` the work sits on. `ObjectHash::ZERO` while the repository has no history. |
| `working_change` | `ChangeId` that the next `NewChange` produces a version of. |
| `snapshot` | Latest `TreeHash` set by `Snapshot` or `UpdateBase`; `None` until then. |
| `sparse` | `SparseProfile { include, exclude }` of directory prefixes. Empty `include` means the whole repo; `exclude` wins over `include`. |
| `head_op` | Id of the latest `Operation`. |

## Operation log and idempotency

Every mutating RPC ends in `WorkspaceSvc::record`. It builds
`Operation { id, workspace, parent: ws.head_op, actor, kind,
resulting_base, resulting_snapshot, at }`, sets `head_op`, and calls
`RepoStore::append_operation`. That is one metadata transaction:
`expect_absent(opi/<op_id>)`, append to `op/<repo>/<ws>/`, set
`ws/<repo>/<ws>`. The `opi` index row is written by a second, separate
`set` after the transaction commits.

The `operation_id` in the request is the idempotency key. An empty id means
the server generates one. A reused id fails the transaction and surfaces as
`AlreadyExists` ("operation … was already applied"). The SDK exposes this
through the `*_with_op` methods on `WorkspaceClient`.

## RPCs

All RPCs live on `WorkspaceSvc`. Reads need `read(root)`. `CreateWorkspace`
needs `write(root)`. Every other mutating RPC loads the record through
`load_mut`, which needs `write(root)` and owner-or-admin.

| RPC | `OperationKind` | Effect |
| --- | --- | --- |
| `CreateWorkspace` | `Create` | `base` resolved from the optional `RootSpec` (trunk head when absent, `ZERO` when the repo is empty; a tree hash is `InvalidArgument`). Fresh `working_change`. |
| `Snapshot` | `Snapshot { tree }` | Sets `snapshot`. The tree must already be stored, else `FailedPrecondition`. |
| `NewChange` | `NewChange { change, base }` | See below. |
| `UpdateBase` | `Rebase { from, to }` | See below. |
| `SetSparseProfile` | `SetSparse { profile }` | Replaces `sparse`. |
| `Handoff` | `Handoff { to }` | `owner := to`. `to` is required. |
| `Undo` | `Undo { undone }` | See below. |
| `ListOperations` | none | Latest `limit` operations (default 50, max 1000), optionally only those before an id. |
| `GetWorkspace`, `ListWorkspaces` | none | `ListWorkspaces` filters by owner; `limit` default 100, max 10 000. |
| `WorkspaceDiff` | none | File diffs from the base root to `snapshot`, through the caller's read filter. |
| `DeleteWorkspace` | none | Deletes the record. Not logged. |

### NewChange

1. `snapshot` must be set, else `FailedPrecondition`.
2. `diff_trees(base root, snapshot)`; an empty diff is `FailedPrecondition`.
3. `write(path)` is checked for every touched path.
4. `version_from_diff` builds a `ChangeVersion` of `working_change` with
   `parents = [base]` (none when `base` is `ZERO`) and `predecessor` = the
   last entry of `change_versions(working_change)`.
5. `commit_version` stores it (see
   [changes and stacks](changes-and-stacks.md)).
6. `base := new hash`, `working_change := ChangeId::new()`.

The next `NewChange` is therefore parented on the previous one, which is
how a workspace builds a stack.

### UpdateBase

The new base must resolve to a version. An empty repository gives
`FailedPrecondition`. If `working_change` already has a version and the
base actually moves, that version is rebased with `rill_core::rebase`. The
rebased version is committed, and `snapshot` becomes its root tree.
Conflicts are recorded in the version and never cause an error (see
[rebase](../03-core-algorithms/rebase.md)). Then `base := new`.

After a `NewChange`, `working_change` is a fresh id with no versions, so
`UpdateBase` only moves `base`.

### Undo

`Undo` reads the latest 1000 operations. It finds `target` (default
`head_op`) and then the target's parent. It sets `base` and `snapshot` to
the parent's `resulting_base` and `resulting_snapshot`, and records a new
`Undo` operation. Undoing the first operation is `FailedPrecondition`. A
target or parent outside the 1000-operation window is `NotFound`. Undo is
itself an operation, so undoing an `Undo` restores the state it replaced.

`owner`, `sparse` and `working_change` are not restored. Undoing a
`NewChange` moves `base` back but keeps the fresh `working_change`, and the
committed version stays in history.

## Persistence

| Key | Value |
| --- | --- |
| `ws/<repo>/<ws>` (`keys::workspace`) | `Workspace` record |
| `op/<repo>/<ws>/<be64 seq>` (`keys::operation_prefix`) | `Operation`, append-only |
| `opi/<repo>/<op_id>` (`keys::operation_index`) | log sequence, for idempotency |

## Limitations

* The `opi` row is written outside the append transaction. A failure
  between the two writes leaves that operation id replayable.
* No RPC returns the original response for a replayed operation id. A
  replayed `NewChange` fails on the empty diff instead of `AlreadyExists`.
* `Undo` sees only the latest 1000 operations and does not restore
  `owner`, `sparse` or `working_change`.
* `Snapshot` checks that the root tree exists. It does not check that the
  tree's blobs exist or that the tree respects the sparse profile. The
  server never filters a snapshot or a `NewChange` diff by profile.
* `DeleteWorkspace` removes only the record. The operation log and index
  rows stay.
* `ListWorkspaces` scans every workspace in the repository and has no
  continuation token.
* The `Describe`, `Resolve` and `Other` operation kinds exist in the type
  but no RPC records them.

## Code

* [rill-types/src/objects/workspace.rs](../../crates/rill-types/src/objects/workspace.rs)
* [rill-server/src/services/workspace.rs](../../crates/rill-server/src/services/workspace.rs)
* [rill-server/src/services/repo/write.rs](../../crates/rill-server/src/services/repo/write.rs) (`commit_version`, `version_from_diff`)
* [rill-store/src/repo.rs](../../crates/rill-store/src/repo.rs) (`append_operation`, `list_operations`)
* [rill-store/src/keys.rs](../../crates/rill-store/src/keys.rs)
* [rill-sdk/src/workspace.rs](../../crates/rill-sdk/src/workspace.rs)
* [rill-cli/src/commands/ws.rs](../../crates/rill-cli/src/commands/ws.rs)

---
← Previous: [Change builder](../03-core-algorithms/change-builder.md) · [Index](../index.md) · Next: [Checkouts](checkouts.md) →
