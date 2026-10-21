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

`rill-store` features: `local` (default, redb plus files), `s3`
(`object_store`) and `fdb` (the FoundationDB metadata store).
`rill-api` enables `s3`; `rill-api/fdb` and `rill-cli/fdb` forward
`fdb`. Without `fdb` nothing needs the FoundationDB client library.
`aws` and `gcp` add the cloud key services. `saml` (on `rill-identity`,
`rill-api` and `rill-cli`) adds SAML sign-in and needs libxml2 and xmlsec1;
see [identity](../05-policy/identity-and-auth.md#saml-20) for the macOS
`PKG_CONFIG_PATH`. CI builds neither.

## FoundationDB locally

The `fdb` feature links `libfdb_c` from FoundationDB 7.3 (the newest line
the `foundationdb` crate supports, feature `fdb-7_3`). On macOS install the
official package from the FoundationDB releases page
(`FoundationDB-7.3.x_arm64.pkg` or `_x86_64.pkg`, checked against its
`.sha256`); it installs the client library, `fdbcli` and a single-node
server started by launchd, with its cluster file at
`/usr/local/etc/foundationdb/fdb.cluster`. On Debian or Ubuntu install the
`foundationdb-clients` and `foundationdb-server` packages.
`fdbcli --exec status` should report `Healthy`; if the database is
unconfigured, run `fdbcli --exec "configure new single ssd"`.

```
cargo test -p rill-store --features fdb,s3              # conformance on FoundationDB
cargo test -p rill-api --features fdb --test grpc    # end to end on FoundationDB
RILL_FDB_CLUSTER_FILE= RILL_DEV_MODE=true cargo run -p rill-cli --features fdb -- \
    api --storage local:/tmp/rill-dev                   # objects on disk, metadata in FoundationDB
```

Keep objects persistent when metadata is: `memory` storage keeps objects in
memory, so after a restart the metadata would point at objects that are
gone. Tests use a fresh `rill-test-<random>` root subspace each and clear
it afterwards, so they never touch a deployment's data.

The macOS package installs `libfdb_c.dylib` as `@rpath/libfdb_c.dylib`.
`rill-cli`'s build script adds an rpath to `/usr/local/lib` (or
`FDB_LIB_DIR`) for `fdb` builds, so `rill-api` starts outside `cargo
run`.

## CI

```mermaid
flowchart LR
    Trig[push to main / pull request] --> Fmt[cargo fmt --all --check]
    Fmt --> Clippy[cargo clippy --workspace --all-targets]
    Clippy --> Test[cargo test --workspace]
    Trig --> Install[install FoundationDB 7.3.77]
    Install --> FClippy[clippy with fdb]
    FClippy --> FTest[conformance + end to end on FoundationDB]
```

`.github/workflows/ci.yml` runs two jobs on `ubuntu-latest`. `check` uses
the stable toolchain with rustfmt and clippy and `Swatinem/rust-cache`.
`RUSTFLAGS=-D warnings` turns every compiler and clippy warning into an
error. No feature flags are passed. With `--workspace`, feature unification
still turns on `s3` for `rill-store`, because `rill-api` requests it. The
git importer test uses the runner's `git` binary.

`foundationdb` installs the FoundationDB client and server packages, whose
version and SHA-256 sums are pinned in the job's `env`, waits for the
single-node cluster to report healthy, then lints the `fdb` code and runs
the store conformance suite and the server's end-to-end test against it.

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
- Reuse before building ([ADR 015](../10-decisions/015-reuse-open-source.md)):
  generic needs come from a mature, permissively licensed library, pinned
  exactly when pre-1.0 and wrapped behind a Rill module. Tests cover the
  integration, not the library. GPL, AGPL, SSPL and source-available code
  is never linked.
- A behaviour change updates the architecture page that owns it.

## Running locally

```
cargo run -p rill-cli --bin rill -- dev --memory   # every role, gRPC :7470, HTTP :7471
cargo run -p rill-cli --bin rill -- --endpoint http://127.0.0.1:7470 repo create demo
cargo run -p rill-cli --bin rill -- --endpoint http://127.0.0.1:7470 checkout demo ./demo
cd demo && echo 'fn main() {}' > main.rs
rill new -m "initial" && rill submit && rill land --wait
```

Inside a checkout, `.rill/checkout.toml` supplies the endpoint and
repository ([cli](../08-clients/cli.md)). `rill api init-config` prints
an annotated default config ([configuration](../07-runtime/configuration.md)).
`rill-mcp --list-tools` prints the MCP tool schemas. `RUST_LOG` controls
logging in all three binaries.

## Limitations

- CI runs on Linux only, with no matrix for other platforms or for the
  `rust-version` minimum.
- No `cargo-deny`, `cargo-audit` or license check.
- There is no `[workspace.lints]` table. Lint levels come from defaults,
  per-crate attributes and the CI `RUSTFLAGS`. Locally, pass `-D warnings`
  to clippy after `--` instead: changing `RUSTFLAGS` rebuilds the whole
  workspace into a second copy of `target/`.
- Some files are over the 400-line guideline: the `review`, `workspace` and
  `admin` gRPC service files and `rill-cli/src/commands/review.rs`.

## Code

- [Cargo.toml](../../Cargo.toml), [rust-toolchain.toml](../../rust-toolchain.toml), [rustfmt.toml](../../rustfmt.toml), [CLAUDE.md](../../CLAUDE.md)
- [.github/workflows/ci.yml](../../.github/workflows/ci.yml)
- [rill-proto/build.rs](../../crates/rill-proto/build.rs), [rill-store/Cargo.toml](../../crates/rill-store/Cargo.toml), [rill-cli/Cargo.toml](../../crates/rill-cli/Cargo.toml)
- [rill-cli/src/server/mod.rs](../../crates/rill-cli/src/server/mod.rs)

---
← Previous: [MCP server](../08-clients/mcp.md) · [Index](../index.md) · Next: [Testing](testing.md) →
