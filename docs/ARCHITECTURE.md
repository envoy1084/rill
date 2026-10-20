# Rill architecture (short form)

The full knowledge base is [architecture/](../architecture/README.md); start
with the [system model](../architecture/system.md). This page is the one-screen
version every contributor reads first: the principles and the invariants that
must hold across all crates.

## Principles (in priority order)

1. The server is the repository. Clients hold bounded caches; every operation
   a client can do is an API call that needs no checkout.
2. Immutable, content-addressed data everywhere (BLAKE3). Mutable state is a
   small set of refs plus append-only logs.
3. Cost scales with the change, not the repo.
4. Stable identity (`ChangeId`), mutable content (`ChangeVersion`).
5. Conflicts are data (`Merge<T>` terms), never errors.
6. Intent and provenance are first-class and signed into every change.
7. Landing is optimistic; verification is continuous (post-land, bisect,
   revert).
8. Agents are principals with their own identity, capabilities and budgets.
9. Build, test and search share the object store.
10. Humans get the ergonomic surface; agents get the structural one. Nothing
    is possible in a UI that is not possible through the API.

## Layering

`rill-types` ← `rill-store` ← `rill-core` ← `rill-policy` / `rill-review` /
`rill-lander` / `rill-search` / `rill-exec` ← `rill-server` ← `rill-sdk` ←
`rill-cli` / `rill-mcp`. Lower crates never depend on higher ones. The binary
crate `rill-cli` additionally links `rill-server`, `rill-store`,
`rill-git-import` and `rill-mcp` (for the `rill-server` binary, `import` and
JSON output); no library crate does.

No crate below `rill-proto` depends on tonic, and only `rill-server` depends
on axum. The library crates (`core`, `policy`, `review`, `lander`, `exec`,
`search`) have no RPC dependency.

## Invariants

* Every write to the metadata store is O(size of change). The root tree of a
  version is derived data, never stored in the version.
* Objects are verified against their hash on every `put`.
* `Merge::simplify` operates on separate add/remove lists, never on the
  interleaved vector.
* Serde types that go through postcard never use `skip_serializing_if`.
* Trunk moves only by compare-and-swap on the trunk ref inside the lander;
  sequence numbers are gap-free.
* A rebase never fails; it may produce a conflicted version.
* The server hot-swaps the `ReviewService` when `SetPolicy` changes the Cedar
  text; `rill-review` is the only consumer of `rill-policy::submit`.
* `rill-lander` does not depend on `rill-review`; the server adapts the two
  through `ReviewHooks`.
* Search indexes blobs by hash, so an unchanged blob is never re-indexed; the
  indexer follows `Landed` events and indexes on demand otherwise.
* One lander per repository per process; landing records are serialised by a
  process-local lock.

## Where to read next

| Question | Page |
| --- | --- |
| What is stored and how is it addressed | [objects](../architecture/data/objects.md), [storage](../architecture/data/storage.md) |
| How a change gets from a working copy to trunk | [workspaces](../architecture/workflow/workspaces.md), [review](../architecture/workflow/review.md), [landing](../architecture/workflow/landing.md) |
| Who may do what | [identity](../architecture/policy/identity-and-authentication.md), [permissions](../architecture/policy/permissions.md), [submit policy](../architecture/policy/submit-policy.md) |
| How the server is put together | [server](../architecture/runtime/server.md), [protocol](../architecture/runtime/protocol.md) |
| What is missing before production | [research](../research/production-readiness/index.md) |
