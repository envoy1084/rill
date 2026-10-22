# Contributing to Rill

Thank you for helping. Rill is pre-release, so formats and APIs change
freely: there is no backward compatibility to keep and no migrations to
write. That makes it a good time to fix things properly rather than work
around them.

## Before you start

- Read the [architecture index](architecture/index.md) and the
  [principles and invariants](architecture/01-overview/principles-and-invariants.md).
  The invariants are not negotiable; a change that breaks one is not merged.
- Fixed design decisions live in [architecture/10-decisions](architecture/10-decisions/README.md).
  Changing one takes a new ADR, so open an issue to discuss it first.
- For anything larger than a bug fix, open an issue describing the problem
  and the approach before writing the code.

## Building

Stable Rust (1.85 or newer, pinned by `rust-toolchain.toml`). No `protoc`
is needed; protobufs compile with `protox`.

```sh
cargo build --workspace --all-targets
cargo nextest run --workspace          # or: cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo deny check                       # licences, advisories, sources
```

Optional features: `fdb` (FoundationDB metadata, needs the FoundationDB
client library), `aws` and `gcp` (cloud key services), `saml` (needs
libxml2 and xmlsec1). With FoundationDB running locally:

```sh
cargo test -p rill-store --features fdb,s3
cargo test -p rill-api --features fdb --test grpc
```

Lint with `-- -D warnings` as above, not with `RUSTFLAGS`, which rebuilds
the workspace into a second copy.

## Where code goes

Crates live in `crates/<group>/<name>`, grouped by layer: `foundation`,
`engine`, `domain`, `server`, `clients`. Tooling lives in `tools/`.
Dependencies only point down the stack described in
[crates and layering](architecture/01-overview/crates-and-layering.md);
lower crates never depend on higher ones.

Inside a crate:

- **One concern per file, one topic per folder.** A `mod.rs` only declares
  modules and re-exports.
- **Small files.** Around 400 lines is a signal to split along a real seam
  (types, algorithm, I/O, errors).
- **Pure logic apart from I/O**, so it can be tested without storage or a
  network. Keep public APIs narrow.
- **Names say what things are.** No `helpers.rs` or `utils.rs`.

## Reuse before writing

For anything that is not Rill's own design, use a mature open-source crate
instead of writing one, and delete the code it replaces
([ADR 015](architecture/10-decisions/015-reuse-open-source.md)).

- Only permissive licences: MIT, BSD, Apache-2.0, ISC, Zlib, Unicode and
  MPL-2.0. Never GPL, AGPL, SSPL or source-available code. `cargo deny`
  enforces this.
- Pin pre-1.0 crates to an exact version and wrap them behind a Rill
  module.
- Test the integration, not the library.
- Declare every dependency once in the root `[workspace.dependencies]`.

## Code style

- Rust 2021. `cargo fmt` is a no-op and clippy is clean.
- Comments are rare: write one only for what the code cannot say (an
  invariant, a non-obvious reason, a safety argument). Public items get a
  one-line doc comment.
- Errors: each crate has an `error.rs` with a `thiserror` enum and a
  `Result` alias. No `unwrap` outside tests unless a comment proves it
  cannot fail.
- Async code uses `tokio`. Blocking work (redb, the filesystem) goes on
  `spawn_blocking`.
- Logging uses `tracing` with structured fields, never `println!` in a
  library.

## Tests

- Test behaviour that matters: an invariant, a contract, a past bug, or an
  edge case that could realistically break. Skip tests that only restate
  the implementation.
- Prefer property tests for algebraic code (merge, diff, trees, encoding)
  and scenario tests for workflows (two users conflict, a land races a
  vote). Name a test for what it guards, e.g.
  `partial_resolution_keeps_remaining_conflict`.
- Unit tests live next to the code. Cross-module tests live in the crate's
  `tests/`, grouped by topic, with shared fixtures in `tests/common/`.
- Every bug fix comes with a test that fails before the fix.

## Documentation

The pages under `architecture/` describe the code as it is. When you change
behaviour, update the page that owns it in the same change, and keep the
reading order in `architecture/index.md` in sync when pages are added or
moved.

## Submitting a change

- Keep each change self-contained: it builds, and the tests and clippy of
  the crates it touches pass.
- Write commit messages as `area: what changed` in the imperative
  (`core: honour context lines in unified diff`), with a body when the
  reason is not obvious.
- Open a pull request against `main`. CI runs formatting, `cargo deny`,
  clippy and the test suite, plus the FoundationDB suites.

By contributing you agree that your contribution is licensed under the
[Apache License, Version 2.0](LICENSE), and that you will follow the
[code of conduct](CODE_OF_CONDUCT.md).
