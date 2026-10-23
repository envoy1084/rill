# Working on Rill

Start with `architecture/index.md` (reading order) and
`architecture/01-overview/principles-and-invariants.md`; the invariants are
not negotiable. Fixed design decisions are in `architecture/10-decisions/`
(ADRs); changing one needs a new ADR. The v2 plan, roadmap and progress are
in the local, untracked `research/scale-plan/`; v1 gaps are in
`research/production-readiness/`. When you change behavior, update the page
that owns it in the same change.

## Development stage

Rill is pre-release. There is no backward compatibility to keep: stored
formats, the wire protocol and APIs change freely, and old data is
recreated rather than migrated. Everything stays "v1" (`rill.v1`, schema 1);
do not introduce v2 names or compatibility shims. The roadmap's "v2" means
"the next design", not a version label.

## Structure

* **One concern per file, one topic per folder.** A folder groups related
  modules; its `mod.rs` only declares and re-exports. Nest folders when a
  topic has sub-topics (`merge/`, `merge/text/`, …) rather than growing flat
  directories of loosely related files.
* **Small files.** Around 400 lines is a signal to split along a real seam
  (types / algorithm / I/O / errors), not an arbitrary one.
* **Clear boundaries.** Pure logic (no I/O) lives apart from code that talks
  to storage or the network, so it can be tested without either. Public APIs
  are narrow: re-export what callers need, keep the rest `pub(crate)`.
* **Reuse before writing.** For anything that is not Rill's own design, use
  a mature open-source library rather than writing one, and delete the
  hand-written version it replaces (ADR 015). Link only permissive
  licences (MIT, BSD, Apache-2.0, ISC, MPL-2.0); never GPL, AGPL, SSPL or
  source-available code. Pin pre-1.0 crates exactly and wrap them behind a
  Rill module. Test the integration, not the library. Inside the
  workspace, look for an existing helper in the crate (and in lower
  crates) before adding one. Shared helpers go in a named module
  (`util/`, `testing/`), never copied between files.
* **Names say what things are.** Modules, types and functions are named for
  their domain meaning; avoid `helpers.rs`, `misc.rs`, `utils2.rs`.

## Code style

* Rust 2021, edition-idiomatic. `cargo clippy --workspace --all-targets` must
  be clean and `cargo fmt` must be a no-op.
* **Comments are rare.** Write one only for what the code cannot say: an
  invariant, a non-obvious reason, a subtle algorithm, a safety argument. Do
  not narrate code, restate names, or leave change-log comments. Public items
  get a one-line doc comment.
* Errors: each crate has an `error.rs` with a `thiserror` enum and a
  `Result<T>` alias. Never `unwrap` outside tests except where a comment
  proves it cannot fail.
* Async: `tokio`. Traits that cross crates use `async_trait`. No blocking
  I/O on async threads; use `spawn_blocking` for redb and filesystem work.
* Logging: `tracing` with structured fields, no `println!` in libraries.
* Serde types that are postcard-encoded must not use `skip_serializing_if`.
* Dependencies come from `[workspace.dependencies]`; add new ones there.

## Tests

* **Test behavior that matters, not lines.** A test earns its place by
  guarding an invariant, a contract, a past bug, or an edge case that could
  realistically break. Skip tests that only restate the implementation,
  check trivial getters, or duplicate another test's coverage.
* Prefer **property tests** for algebraic code (merge, diff, trees,
  encoding) and **scenario tests** for workflows (two users conflict, a land
  races a vote). Name tests for the behavior they guard
  (`partial_resolution_keeps_remaining_conflict`), not the function.
* Units live next to the code (`#[cfg(test)] mod tests`); cross-module
  behavior lives in the crate's one integration-test binary,
  `tests/<name>/main.rs`, a module per topic (`tests/<name>/merge.rs`)
  with shared fixtures in `tests/<name>/common/`. Do not add a test
  binary per file: each links the whole dependency tree.
* **Every bug fix adds a test** that fails before the fix.

## Commits

* Commit after each meaningful, self-contained change, directly on `main`
  (no feature branches). Do not push unless asked. Every commit builds,
  and the tests and clippy of the crates it touches pass (see "Checks"
  below); the full sweep runs at the end of a phase, and anything it finds
  is fixed in a follow-up commit.
* Message format: `area: what changed` in the imperative
  (`core: honour context lines in unified diff`), with a body when the why
  is not obvious.

## Keeping docs and progress current

* `research/` is local planning material and is **not tracked** by git
  (it is in `.gitignore`). Never commit it and never link to it from
  tracked docs.
* **Progress:** as roadmap work completes, update
  `research/scale-plan/progress.md`: the current phase, each work item's
  status (`todo` / `in progress` / `done`) with the commit that finished it,
  exit-gate results, and anything deferred or discovered. Update it at the
  end of every work session, not only at phase boundaries.
* **Architecture docs:** when a major feature or component lands (or v1
  behaviour changes), rewrite the owning page under `architecture/` in the
  same change. Keep pages short, accurate to the code, and keep the reading
  order in `architecture/index.md` and each page's previous/next footer in
  sync when pages are added, moved or removed.

## Layering

`rill-types` ← `rill-store` ← `rill-events` / `rill-jobs` / `rill-core` ←
`rill-policy` / `rill-review` / `rill-lander` / `rill-search` / `rill-exec` ←
`rill-identity` ← `rill-runtime` ← `rill-api` ← `rill-sdk` ← `rill-cli` /
`rill-mcp`, with `rill-server` (the services' binary) above `rill-api`,
`rill-cache` beside the server crates on `rill-policy` and
`rill-merge-syntax` just above `rill-core`. Lower crates never depend on
higher ones. The client binaries (`rill-cli`, `rill-mcp`) never link the
server crates (`rill-api`, `rill-runtime`, `rill-cache`, the domain
services); CI checks it. Tooling crates (`tools/`: `rill-sim`, `rill-bench`,
`rill-load`, `fuzz`) may depend on anything; nothing depends on them.

Crates live in `crates/<group>/<name>` (groups `foundation`, `engine`,
`domain`, `server`, `clients`); the folder drops the `rill-` prefix the
package name keeps. A new crate goes in the group matching its layer. Test-only hooks in library crates live
behind a `testing` feature.

## Checks

Tiered, so a phase spends its time on work rather than rebuilds:

* **Per commit:** `cargo build --workspace --all-targets`, then
  `cargo nextest run -p <touched crates>` (or the touched test binary,
  e.g. `-p rill-api --test grpc access`) and
  `cargo clippy -p <touched crates> --all-targets -- -D warnings`.
* **When a `.proto` changes:** `sdks/generate.sh` (lints, regenerates the
  API reference and the SDKs' code; needs `buf`, `protoc` 36.2,
  `protoc-gen-doc`, `protoc-gen-go`, `protoc-gen-connect-go`, Node with
  pnpm, and `uv`), then `sdks/conformance/run.sh`.
* **When TypeScript changes** (`sdks/ts`, `ide/vscode`, one pnpm
  workspace at the root): `pnpm -r typecheck && pnpm -r build && pnpm -r test`.
  Tool versions live in the `pnpm-workspace.yaml` catalog; use pnpm, never
  npm, and add dependencies with `catalog:` versions.
* **End of a work item:** `cargo clippy --workspace --all-targets -- -D warnings`.
* **End of a phase (the full sweep):** `cargo fmt --check`,
  `cargo nextest run --workspace`, `cargo test --workspace --doc`, clippy
  with the deployment features (`-p rill-server --features fdb,aws,gcp,saml`),
  and the FoundationDB suites below.
* Build feature combinations (`fdb`, `saml`, cloud key services) and
  `--release` only in the full sweep or for load runs. Run long gates
  (image builds, staging, load runs) in the background while other work
  continues.

## CI

`.github/workflows/ci.yml` has two jobs. `check` runs `cargo fmt --check`,
`cargo deny check` (licences, advisories, sources per `deny.toml`),
`cargo clippy -D warnings`, `cargo test --workspace` (with `fuse3`
installed, so FUSE tests mount), then regenerates
everything derived from the protos (`sdks/generate.sh`: `buf lint`, the
API reference, the Go, TypeScript and Python SDKs' code), failing on any
diff, and runs the SDK conformance suites against `rill dev`
(`sdks/conformance/run.sh`). `foundationdb`
installs a pinned FoundationDB (version and package checksums in the job's
env) and runs the store conformance suite and the end-to-end server test
with the `fdb` feature. Keep it that small; anything else is run locally or
added only when it catches real problems.

The daemon's FUSE tests (`rill-daemon/tests/daemon/lazy.rs`) mount only on
Linux; locally run them in the OrbStack machine `rill-fuse` with a
separate target directory:
`orb -m rill-fuse bash -lc 'cd <repo> && CARGO_TARGET_DIR=$HOME/target cargo test -p rill-daemon'`.

Locally, with FoundationDB running:
`cargo test -p rill-store --features fdb,s3` and
`cargo test -p rill-api --features fdb --test grpc`. Run lint checks as
`cargo clippy ... -- -D warnings`, not with `RUSTFLAGS`, which rebuilds
the whole workspace into a second copy.
