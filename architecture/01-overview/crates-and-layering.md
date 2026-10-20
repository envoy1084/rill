# Crates and layering

Rill is one Cargo workspace of fourteen crates under `crates/`. Each crate owns
one boundary, and dependencies only point down the stack, so the algorithms
and policy crates stay free of RPC and storage-layout concerns.

## Crate map

| Crate | Owns | Does not own |
| --- | --- | --- |
| `rill-types` | Object model, ids, BLAKE3 hashing, canonical encoding, `Merge<T>` | I/O, policy decisions |
| `rill-store` | `ObjectStore` / `MetaStore` traits, keyspace, backends, typed `RepoStore`, trunk CAS | Diff, merge, authorization |
| `rill-core` | Diff, three-way merge, trees, rebase, ancestry graph, derived data, blob chunking, `ChangeBuilder` | Storage layout, RPC |
| `rill-policy` | Risk classification, Cedar submit rules, ReBAC, Biscuit capability tokens, signing | Enforcement points (the server owns those) |
| `rill-review` | Labels, votes, threads, interdiff, stacks, submit evaluation, review agents | Landing |
| `rill-lander` | Queue, rebase onto trunk, CAS land, landing records, post-land verification, bisect, revert | Review state (through `ReviewHooks`), execution (through `Verifier`) |
| `rill-search` | Trigram index, symbol extraction, query execution | Permission filtering (the server passes a `PathFilter`) |
| `rill-exec` | Action cache, sandbox materialization, verify plans | Scheduling policy |
| `rill-git-import` | One-way git history import onto trunk | Export, mirroring |
| `rill-proto` | `rill.v1` protobufs, generated tonic services, domain/wire conversion | Handlers |
| `rill-server` | Authentication, `RepoContext`, gRPC handlers, object HTTP protocol, event bus, workers, config | Client-side state |
| `rill-sdk` | Typed client, blob upload, local checkout (`.rill/`), status, sync, stack helpers | User interface |
| `rill-mcp` | stdio JSON-RPC tool surface over the SDK for agents; `rill-mcp` binary | Anything the SDK cannot do |
| `rill-cli` | `rill` and `rill-server` binaries, output formats, exit codes | Business rules |

Tooling crates, never depended on by product crates: `rill-sim` (seeded
fault simulation, [simulation](../09-engineering/simulation.md)),
`rill-bench` and `rill-load` (benchmarks and load generation,
[performance](../09-engineering/performance.md)), and `fuzz/` (a separate
`cargo-fuzz` workspace).

## Dependency graph

Arrows point from a crate to what it depends on, as declared in each
`crates/*/Cargo.toml` (normal dependencies only).

```mermaid
flowchart BT
  types[rill-types]
  store[rill-store] --> types
  core[rill-core] --> store
  policy[rill-policy] --> store
  review[rill-review] --> core & policy
  lander[rill-lander] --> core
  search[rill-search] --> core
  exec[rill-exec] --> core
  import[rill-git-import] --> core
  proto[rill-proto] --> types
  server[rill-server] --> review & lander & search & exec & proto
  sdk[rill-sdk] --> proto & core
  mcp[rill-mcp] --> sdk & core
  cli[rill-cli] --> sdk & mcp & server & store & import
```

## Layering rules

The declared order is `rill-types` <- `rill-store` <- `rill-core` <-
`rill-policy` / `rill-review` / `rill-lander` / `rill-search` / `rill-exec`
<- `rill-server` <- `rill-sdk` <- `rill-cli` / `rill-mcp`. Lower crates never
depend on higher ones. The manifests refine that order in a few places:

- `rill-policy` depends on `rill-types` and `rill-store` only, not on
  `rill-core`. `rill-review` depends on both `rill-core` and `rill-policy`.
- `rill-lander` depends on neither `rill-review` nor `rill-exec`. The server
  adapts them through the lander's `ReviewHooks` and `Verifier` traits
  (`ReviewHooksAdapter`, `ExecVerifier`).
- `rill-proto` depends only on `rill-types`. It is the wire layer between
  the domain crates and anything that speaks gRPC.
- `rill-git-import` sits beside the other `rill-core` consumers. It writes
  through `RepoStore` directly and needs no server.
- `rill-sdk` depends on `rill-proto` and `rill-core`, not on `rill-server`.
  It is a client of the wire surface, not of the server crate.
- `rill-mcp` depends on `rill-sdk` (and `rill-types`, `rill-core`). It uses
  `rill-proto` and `tonic` only as dev-dependencies for tests.
- The binary crate `rill-cli` additionally links `rill-server` (for the
  `rill-server` binary), `rill-store` and `rill-git-import` (for
  `rill import git`), and `rill-mcp` (shared JSON encoding, error codes,
  grep helper). No library crate links those.
- The tooling crates `rill-sim` and `rill-load` link `rill-server` to reuse
  its wiring; they sit above every product crate and nothing depends on
  them. `rill-types` exposes its test strategies and clock overrides only
  under the `testing` feature.

RPC and HTTP dependencies:

- `tonic` appears in `rill-proto`, `rill-server`, `rill-sdk` and `rill-cli`.
  No crate below `rill-proto` depends on it.
- `axum` appears only in `rill-server`, which serves the object protocol and
  health endpoints.
- The library crates `rill-core`, `rill-policy`, `rill-review`,
  `rill-lander`, `rill-exec` and `rill-search` have no RPC dependency.

Every external dependency is declared once in the root
`[workspace.dependencies]` table; crates reference it with
`{ workspace = true }`. See [development](../09-engineering/development.md)
for build profiles and features.

## Limitations

- Layering is enforced by review only; there is no `cargo deny` or
  dependency-graph check.

## Code

- [Workspace manifest](../../Cargo.toml)
- [rill-policy/Cargo.toml](../../crates/rill-policy/Cargo.toml)
- [rill-lander/Cargo.toml](../../crates/rill-lander/Cargo.toml)
- [rill-server/Cargo.toml](../../crates/rill-server/Cargo.toml)
- [rill-sdk/Cargo.toml](../../crates/rill-sdk/Cargo.toml)
- [rill-mcp/Cargo.toml](../../crates/rill-mcp/Cargo.toml)
- [rill-cli/Cargo.toml](../../crates/rill-cli/Cargo.toml)
- [Lander adapters](../../crates/rill-server/src/workers/mod.rs)

---
← Previous: [Principles and invariants](principles-and-invariants.md) · [Index](../index.md) · Next: [Glossary](glossary.md) →
