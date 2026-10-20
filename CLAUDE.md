# Working on Rill

Read `docs/ARCHITECTURE.md` first; the invariants there are not negotiable.
The full knowledge base is `architecture/` (one page per boundary, start at
`architecture/system.md`); gaps and design directions are in
`research/production-readiness/`. When you change behavior, update the page
that owns it in the same change.

## Code style

* Rust 2021, edition-idiomatic. `cargo clippy --all-targets` must be clean
  and `cargo fmt` must be a no-op.
* Small modules. One concern per file; a file over ~400 lines is a signal to
  split. Group related modules in a folder with a `mod.rs` that only
  declares and re-exports.
* Comments explain what is not obvious from the code: an invariant, a reason
  for an unusual choice, a non-trivial algorithm. Do not narrate what a line
  does. Every public item gets a one-line doc comment.
* Errors: each crate has an `error.rs` with a `thiserror` enum and a
  `Result<T>` alias. Never `unwrap` outside tests except where a comment
  proves it cannot fail.
* Async: `tokio`. Traits that cross crates use `async_trait`. No blocking
  I/O on async threads; use `spawn_blocking` for redb and filesystem work.
* Logging: `tracing` with structured fields, no `println!` in libraries.
* Serde types that are postcard-encoded must not use `skip_serializing_if`.
* Tests live next to the code (`#[cfg(test)] mod tests`) for units and in
  `tests/` for cross-module behavior. Every bug fix adds a test.
* Dependencies come from `[workspace.dependencies]`; add new ones there.

## Layering

`rill-types` ← `rill-store` ← `rill-core` ← `rill-policy` / `rill-review` /
`rill-lander` / `rill-search` / `rill-exec` ← `rill-server` ← `rill-sdk` ←
`rill-cli` / `rill-mcp`. Lower crates never depend on higher ones.
