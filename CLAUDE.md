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
* **Reuse before writing.** Look for an existing helper in the crate (and in
  lower crates) before adding one. Shared helpers go in a named module
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
  behavior lives in the crate's `tests/`, grouped by topic
  (`tests/merge/`, `tests/workflow/`) with shared fixtures in
  `tests/common/`.
* **Every bug fix adds a test** that fails before the fix.

## Commits

* Commit after each meaningful, self-contained change, directly on `main`
  (no feature branches). Do not push unless asked. The tree builds and
  tests pass at every commit.
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

`rill-types` ← `rill-store` ← `rill-core` ← `rill-policy` / `rill-review` /
`rill-lander` / `rill-search` / `rill-exec` ← `rill-server` ← `rill-sdk` ←
`rill-cli` / `rill-mcp`. Lower crates never depend on higher ones. Tooling
crates (`rill-sim`, `rill-bench`, `rill-load`, `fuzz/`) may depend on
anything; nothing depends on them. Test-only hooks in library crates live
behind a `testing` feature.

## CI

`.github/workflows/ci.yml` runs `cargo fmt --check`, `cargo clippy -D
warnings` and `cargo test --workspace`. Keep it that small; anything else is
run locally or added only when it catches real problems.
