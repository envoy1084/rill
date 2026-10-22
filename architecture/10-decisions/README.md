# Architecture decisions

Decisions that shape Rill v2 and that later work may rely on without
re-arguing them. Each record states the context, the decision, its
consequences, the alternatives rejected and what evidence would reopen it.
The plan that motivated them (its architecture and target-workload notes)
lives in the local, untracked `research/scale-plan/` folder. These records
describe the target design; the rest of this knowledge base describes the
current code.

To change a decision, add a new record that supersedes it; do not edit an
accepted one beyond fixing errors. Status is one of `accepted`,
`superseded by NNN`.

| # | Decision | Status |
| --- | --- | --- |
| [001](001-foundationdb-metadata.md) | FoundationDB is the production metadata store | accepted |
| [002](002-content-in-object-store.md) | File content lives only in the object store | accepted |
| [003](003-single-sequencer.md) | One sequencer per repository; trunk is never sharded | accepted |
| [004](004-queues-and-events-on-fdb.md) | Job queues, event log and leases live in FoundationDB | accepted |
| [005](005-rill-v2-api.md) | `rill.v2` is a clean break, served as gRPC and Connect | superseded by 017 for naming and transport |
| [006](006-workspace-authoritative.md) | The server workspace is authoritative; checkouts are caches | accepted |
| [007](007-reapi-execution.md) | Execution speaks REAPI v2 with BLAKE3 | accepted |
| [008](008-derived-data-async.md) | Derived data is asynchronous except a land-critical set | accepted |
| [009](009-blake3.md) | Keep domain-separated BLAKE3 | superseded by 016 for raw content |
| [010](010-multi-tenancy.md) | Multi-tenancy is built into the keyspace | accepted |
| [011](011-idempotent-api.md) | Every mutation is idempotent; every list is paginated | accepted |
| [012](012-rust-server-generated-clients.md) | Rust on the server; generated clients elsewhere | accepted |
| [013](013-p256-envelope-token-keys.md) | P-256 everywhere, with token keys wrapped by a key service | accepted |
| [014](014-objects-per-organization.md) | Objects are stored and deduplicated per organization | accepted |
| [015](015-reuse-open-source.md) | Reuse mature open source; build only what is Rill's | accepted |
| [016](016-plain-blake3-for-content.md) | Raw content is plain BLAKE3 | accepted |
| [017](017-api-in-place-on-connect.md) | The API is redesigned in place as `rill.v1`, on connect-rust | accepted |

## Template

```markdown
# NNN · Title

Status: accepted

## Context
## Decision
## Consequences
## Alternatives rejected
## Revisit when
```

---
← Previous: [Performance](../09-engineering/performance.md) · [Index](../index.md)
