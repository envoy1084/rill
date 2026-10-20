# Development

Rill is one Cargo workspace of fourteen crates under `crates/`. It uses Rust
2021 with resolver 2. This page covers how the repository is laid out, how
it builds, and what CI checks. The crate dependency rules are on
[crates and layering](../01-overview/crates-and-layering.md).

## Workspace layout

| Path | Contents |
| --- | --- |
| `Cargo.toml` | members, `[workspace.package]`, `[workspace.dependencies]`, build profiles |
| `rust-toolchain.toml` | toolchain channel and components |
| `rustfmt.toml` | `edition = "2021"`, `max_width = 100` |
| `CLAUDE.md` | contributor rules: code style, errors, async, layering |
| `docs/` | `ARCHITECTURE.md` (pointer to this knowledge base), `GETTING_STARTED.md`, `USER_GUIDE.md` |
| `architecture/` | this knowledge base, start at [index](../index.md) |
| `crates/<name>/src` | one crate; `lib.rs` declares modules, `error.rs` holds the error enum |
| `crates/<name>/tests` | cross-module tests ([testing](testing.md)) |
| `crates/rill-proto/proto/rill/v1` | the `.proto` sources |
| `.github/workflows/ci.yml` | the CI pipeline |
| `fuzz/` | `cargo-fuzz` targets, a separate workspace excluded from the main one |

`.gitignore` excludes `/target`, `.rill/` checkouts, `*.redb` files and the
local `research/` notes.

## Toolchain

`rust-toolchain.toml` pins `channel = "stable"` with the `rustfmt` and
`clippy` components. `[workspace.package]` sets `rust-version = "1.85"`,
version `0.1.0` and license Apache-2.0.

No `protoc` is needed. `rill-proto/build.rs` compiles the protobufs with
`protox` and writes a descriptor set that the server serves for reflection
([api surface](../07-runtime/api-surface.md)).

`crates/rill-proto/buf.yaml` holds the lint and breaking-change rules for
`rill.v2` (buf's `STANDARD` lint set and `FILE` breaking rules); `rill.v1`
is exempt. Run `buf lint` from `crates/rill-proto` (`npx @bufbuild/buf lint`
works without installing buf). It is not part of CI.

## Build profiles

| Profile | Settings | Reason |
| --- | --- | --- |
| `dev` | `opt-level = 1`, `debug = "line-tables-only"` | tests and the dev server run at usable speed; full debuginfo made each test binary several hundred MB |
| `dev.package."*"` | `opt-level = 2` | dependencies (BLAKE3, zstd, redb, regex, cedar) run near release speed in debug builds |
| `test` | `debug = "line-tables-only"` | same size control for test binaries |
| `release` | `lto = "thin"`, `codegen-units = 4`, `debug = 1` | cross-crate inlining with bounded link time; line tables kept for backtraces |

`rill-store` features: `local` (default, redb plus files) and `s3`
(`object_store`). `rill-server` enables `s3`.

## CI

```mermaid
flowchart LR
    Trig[push to main / pull request] --> Fmt[cargo fmt --all --check]
    Fmt --> Clippy[cargo clippy --workspace --all-targets]
    Clippy --> Test[cargo test --workspace]
```

`.github/workflows/ci.yml` runs one `check` job on `ubuntu-latest`. It uses
the stable toolchain with rustfmt and clippy and `Swatinem/rust-cache`.
`RUSTFLAGS=-D warnings` turns every compiler and clippy warning into an
error. No feature flags are passed. With `--workspace`, feature unification
still turns on `s3` for `rill-store`, because `rill-server` requests it. The
git importer test uses the runner's `git` binary.

## Conventions

`CLAUDE.md` is the authority. In short:

- `cargo clippy --all-targets` is clean and `cargo fmt` is a no-op.
- One concern per file. About 400 lines is the signal to split. A folder's
  `mod.rs` only declares and re-exports.
- Every public item has a one-line doc comment. `rill-sdk`, `rill-cli` and
  `rill-mcp` enforce this with `#![warn(missing_docs)]`.
- Each crate has `error.rs` with a `thiserror` enum and a `Result<T>` alias.
  No `unwrap` outside tests without a comment proving it cannot fail.
- Async is `tokio`. Redb and filesystem work go through `spawn_blocking`.
  Logs use `tracing` with no `println!` in libraries.
- Postcard-encoded types never use `skip_serializing_if`.
- Every external dependency is declared once in `[workspace.dependencies]`.
  Crates use `{ workspace = true }`.
- A behaviour change updates the architecture page that owns it.

## Running locally

```
cargo run -p rill-cli --bin rill-server -- --dev     # memory storage, gRPC :7470, HTTP :7471
cargo run -p rill-cli --bin rill -- --endpoint http://127.0.0.1:7470 repo create demo
cargo run -p rill-cli --bin rill -- --endpoint http://127.0.0.1:7470 checkout demo ./demo
cd demo && echo 'fn main() {}' > main.rs
rill new -m "initial" && rill submit && rill land --wait
```

Inside a checkout, `.rill/checkout.toml` supplies the endpoint and
repository ([cli](../08-clients/cli.md)). `rill-server init-config` prints
an annotated default config ([configuration](../07-runtime/configuration.md)).
`rill-mcp --list-tools` prints the MCP tool schemas. `RUST_LOG` controls
logging in all three binaries.

## Limitations

- CI runs on Linux only, with no matrix for other platforms or for the
  `rust-version` minimum.
- No `cargo-deny`, `cargo-audit` or license check.
- There is no `[workspace.lints]` table. Lint levels come from defaults,
  per-crate attributes and the CI `RUSTFLAGS`.
- Some files are over the 400-line guideline: `rill-store/src/repo.rs`, the
  `review`, `workspace` and `admin` gRPC service files, `rill-server/src/config.rs`
  and `rill-cli/src/commands/review.rs`.

## Code

- [Cargo.toml](../../Cargo.toml), [rust-toolchain.toml](../../rust-toolchain.toml), [rustfmt.toml](../../rustfmt.toml), [CLAUDE.md](../../CLAUDE.md)
- [.github/workflows/ci.yml](../../.github/workflows/ci.yml)
- [rill-proto/build.rs](../../crates/rill-proto/build.rs), [rill-store/Cargo.toml](../../crates/rill-store/Cargo.toml), [rill-cli/Cargo.toml](../../crates/rill-cli/Cargo.toml)
- [rill-cli/src/server/mod.rs](../../crates/rill-cli/src/server/mod.rs)

---
← Previous: [MCP server](../08-clients/mcp.md) · [Index](../index.md) · Next: [Testing](testing.md) →
