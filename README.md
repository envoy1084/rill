# Rill

**Source control for codebases written by humans and AI agents together.**

One server owns each repository. Every person and every agent works in a
private server-side workspace, submits changes for review, and the lander puts
them on a single numbered trunk (`trunk@18422931`). Review, landing,
verification, search and permissions are part of the system, not products
bolted on around it.

Rill is written in Rust and is not git-compatible. A one-way importer brings a
git history in, and a one-way mirror publishes trunk to a git branch.

> **Status: pre-release.** Rill is under active development. Stored formats,
> the wire protocol and the CLI change without migration between commits. Do
> not use it for anything you cannot recreate.

## Why Rill

- **Changes, not commits.** Every change keeps a stable id across amend and
  rebase, so review state follows the work instead of a hash.
- **Conflicts are data.** A rebase never fails: conflicts are recorded inside
  the change and can be reviewed, handed off or resolved later. Adjacent edits
  to imports, functions and other entities merge structurally
  ([weave](https://crates.io/crates/weave-core)).
- **One trunk, landed in order.** The sequencer lands non-overlapping changes
  in parallel, speculates on the queue, compare-and-swaps trunk and numbers
  every landing without gaps. Verification continues after landing, with
  batching, bisect and automatic revert.
- **Agents are principals.** Agents have registered identities, sponsors,
  budgets, narrow expiring capability tokens and a kill switch. Every change
  records which model and session wrote it. An MCP server gives an agent 27
  tools to read, edit, submit, review and land without a checkout.
- **Policy is part of the server.** Per-directory ownership (relationship
  tuples), Cedar submit rules over a computed risk class, secret scanning on
  every stored version, a hash-chained audit log, and objects sealed at rest
  under per-organization keys.
- **Built for scale.** Content-addressed storage (BLAKE3, FastCDC chunking) on
  S3, metadata in FoundationDB, writes that cost the size of the change rather
  than the repository, trigram code search and a regional read cache.

## Quick start

Requires stable Rust (1.85 or newer). No `protoc` is needed.

```sh
cargo build --release -p rill-cli -p rill-mcp
export PATH="$PWD/target/release:$PATH"

rill dev                                  # every role in one process, no auth, :7470

rill repo create app
rill checkout app ~/app && cd ~/app
echo 'fn main() {}' > main.rs
rill new -m "initial"                     # snapshot into a new change ch_…
rill submit
rill review vote <change> Review 2
rill review vote <change> Verified 1
rill land --wait                          # Landed trunk@1
rill log
```

`rill --help` lists every command; `rill <command> --help` explains one.

### Running a server

```sh
rill api init-config > rill.toml          # annotated defaults
rill api --config rill.toml               # gRPC :7420, HTTP :7421
```

Storage is `memory`, `local:<dir>` (objects as files, metadata in redb) or
S3 with metadata in FoundationDB. The first token comes from
`rill api mint-token`; people then sign in through OpenID Connect or SAML
(`rill login`). `RILL_*` environment variables override individual keys. The
[deploy](deploy/README.md) folder has a Docker image, a Helm chart and a
staging cluster for Docker Compose.

### Connecting an agent

```sh
rill-mcp --endpoint http://host:7420 --token <agent-token> --repo app
```

Register that command as an MCP server. `rill_apply_edits` edits files against
a base and creates a change in one call; `rill_submit`, `rill_request_land` and
`rill_land_status` run the same review loop a person runs.

## Repository layout

```
crates/
  foundation/   types, proto, store, events, jobs     object model, wire format, storage
  engine/       core, merge-syntax                    diff, merge, trees, rebase, history
  domain/       policy, review, lander, search,       the subsystems with their own rules
                exec, identity, git-import
  server/       runtime, api, cache                   a server process and its API
  clients/      sdk, cli, mcp                         what people and agents run
tools/          sim, bench, load, fuzz                simulation, benchmarks, load, fuzzing
architecture/   how Rill works, page by page, with the design decisions
deploy/         Docker image, Helm chart, compose cluster, dashboards
spec/           TLA+ specifications checked with TLC
```

Each crate folder drops the `rill-` prefix of its package name
(`crates/domain/lander` is `rill-lander`). Dependencies point only down this
list; [crates and layering](architecture/01-overview/crates-and-layering.md)
has the full map.

## Documentation

The [architecture knowledge base](architecture/index.md) describes the code as
it is: the data model, the algorithms, the review and landing workflow, policy,
the runtime and the clients, with a reading order. Start with the
[system model](architecture/01-overview/system-model.md) and the
[principles and invariants](architecture/01-overview/principles-and-invariants.md).
Fixed design decisions are recorded as [ADRs](architecture/10-decisions/README.md).

User documentation will be written once the design settles.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to build, test and submit
changes, and [SECURITY.md](SECURITY.md) to report a vulnerability. Everyone
taking part is expected to follow the [code of conduct](CODE_OF_CONDUCT.md).

## License

Rill is licensed under the [Apache License, Version 2.0](LICENSE). Unless you
state otherwise, any contribution you submit for inclusion in Rill is licensed
under the same terms, without any additional terms or conditions.
