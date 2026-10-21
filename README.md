# Rill

**Source control for codebases written by humans and AI agents together.**

One server owns the repository. Every person and every agent works in a private
server-side workspace, submits changes for review, and the lander puts them on a
single numbered trunk. Review, landing, verification, search and permissions are
part of the system, not products bolted on.

Rill is written from scratch in Rust and is not git-compatible. A one-way
importer exists for migrating a git history; that is the whole bridge.

## What it does

- Stores every blob, tree and change version content-addressed (BLAKE3) and
  makes every write O(size of the change), not of the repository.
- Gives each change a stable id across amend and rebase, so review state follows
  the work and never the hash.
- Records conflicts as data inside a change instead of aborting; a rebase never
  fails, and conflicted files can be reviewed, handed off, or resolved later.
- Runs review as labels and votes evaluated by Cedar policy against a computed
  risk class, with per-directory ownership and unresolved-thread blocking.
- Lands optimistically: rebase onto trunk head, compare-and-swap, gap-free
  sequence numbers (`trunk@18422931`), then verifies continuously after landing
  with batching, bisect and automatic revert.
- Treats agents as principals: attested identity, narrow-only capability tokens,
  sponsor, and signed provenance (model, session) on every change. 27 MCP tools
  let an agent read, edit, submit, review and land without a checkout.
- Serves trigram code search and symbol lookup from the same object store,
  filtered by what the caller may read.

See the [system model](architecture/01-overview/system-model.md) before extending landing,
policy or storage.

## Quick start

```sh
cargo build --release --workspace
export PATH="$PWD/target/release:$PATH"

rill dev                                       # every role, no auth, :7470 / :7471

rill repo create app
rill checkout app ~/app && cd ~/app
echo 'fn main() {}' > main.rs
rill new -m "initial"                          # snapshot → change ch_…
rill submit
rill review vote <ch> Review 2 && rill review vote <ch> Verified 1
rill land --wait                               # Landed trunk@1
rill log
```

The [user guide](docs/USER_GUIDE.md) walks through the daily loop, syncing,
conflicts, stacks, agents and team setup. [Getting started](docs/GETTING_STARTED.md)
covers server configuration.

## Repository

Rust 2021 workspace; `cargo build --workspace` needs Rust 1.85+ and no `protoc`
(protobufs compile with `protox`).

| Crate | Purpose |
| --- | --- |
| [rill-types](crates/rill-types) | Object model, ids, hashing, `Merge<T>` conflict algebra |
| [rill-store](crates/rill-store) | Object and metadata store traits; memory, local (redb + files), S3, FoundationDB |
| [rill-core](crates/rill-core) | Diff, three-way merge, trees, rebase, ancestry graph, derived data, blob chunking |
| [rill-policy](crates/rill-policy) | Risk classification, Cedar submit rules, ReBAC, Biscuit capability tokens, signing |
| [rill-review](crates/rill-review) | Labels, votes, threads, interdiff, stacks, submit evaluation, review agents |
| [rill-lander](crates/rill-lander) | Landing pipeline, sequence numbers, post-land verification, bisect, revert |
| [rill-search](crates/rill-search) | Trigram index and symbol extraction |
| [rill-exec](crates/rill-exec) | Verification runner with a content-addressed action cache |
| [rill-proto](crates/rill-proto) | `rill.v1` protobufs and generated tonic services |
| [rill-events](crates/rill-events) | Reading the event log: resumable, filtered tails and retention |
| [rill-jobs](crates/rill-jobs) | Durable job queues with leased claims and a worker runtime |
| [rill-api](crates/rill-api) | gRPC API, object HTTP protocol, authentication, watch streams, workers |
| [rill-sdk](crates/rill-sdk) | Typed client and the local checkout |
| [rill-cli](crates/rill-cli) | The `rill` binary: client commands, `rill dev` and `rill api` |
| [rill-mcp](crates/rill-mcp) | MCP server exposing the SDK to agents |
| [rill-git-import](crates/rill-git-import) | One-way importer from git |

The [architecture knowledge base](architecture/index.md) documents each
boundary: data model, algorithms, workflow, policy, runtime and integrations.
The fixed design decisions for v2 are in
[architecture/10-decisions](architecture/10-decisions/README.md).

## Running a server

```sh
rill api init-config > rill.toml         # annotated defaults
rill api --config rill.toml              # --roles api,workers by default
```

Storage is `memory`, `local:<dir>` (objects as files, metadata in redb) or
`s3://bucket/prefix` (objects on S3, metadata in redb). Every token is a
Biscuit signed by the instance's P-256 keys, which a key service (a local
master key, Vault, AWS KMS or Google Cloud KMS) wraps; the first comes from
`rill api mint-token`, narrower ones from `rill admin token issue`. Per-directory permissions are relationship tuples
(`rill admin relation add dir:/billing owner principal:<id>`). Submit rules are
Cedar policies (`rill admin policy set <repo>/submit`). `RILL_*` environment
variables override individual keys.

## For agents

```sh
rill-mcp --endpoint http://host:7470 --token <agent-token> --repo app
```

Register that command as an MCP server. `rill_apply_edits` edits files against
a base and creates a change in one call; `rill_submit`, `rill_request_land` and
`rill_land_status` run the same review loop a human runs. Issue agents narrow,
expiring tokens: `rill admin token issue --for agent:fixer --repo app --verbs
read,write,review --paths src/ --expires 4h`.

## Development

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --check
```

Dev and test profiles use `debug = "line-tables-only"` to keep `target/` near
9 GB. [CLAUDE.md](CLAUDE.md) states the code conventions and layering rule;
[development](architecture/09-engineering/development.md) and
[testing](architecture/09-engineering/testing.md) describe the workspace.

## Status

Version 1. Every crate above is implemented and tested in-process (357 tests).
Not production-ready: S3 and FoundationDB backends are untested against real
services, there is no identity provider sign-in yet, execution is unsandboxed, there is no web UI, no virtual
filesystem client, and structural merge is a hook. Rill is being rebuilt as v2
for large monorepos shared by many agents and humans; the decisions behind it
are in [architecture/10-decisions](architecture/10-decisions/README.md).

License: Apache-2.0.
