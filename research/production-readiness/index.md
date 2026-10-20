# Production readiness

What stands between Rill v1 and a deployment a team can trust with its code.
Each brief states the current implementation with source references, the
concrete ways it fails in production, testable requirements, a recommended
design with alternatives, and an ordered work breakdown sized S/M/L/XL.

The architecture pages describe the code as it is; these pages describe what
is missing. When a gap closes, delete its brief and update the owning
architecture page.

## Read this first

Five findings from the audit change the risk picture more than any feature
gap. Each is detailed in its brief.

| Finding | Brief |
| --- | --- |
| No TLS on either listener; the SDK cannot dial `https`. Every token crosses the network in the clear. | [16](16-security-hardening.md) |
| Verification steps from a repository's `.rill/verify.toml` run as host processes with the server's user and network. Any writer has remote code execution on the server when verification is enabled. | [07](07-sandboxed-execution.md), [06](06-operations-and-observability.md) |
| Checkout writes through symlinks and materializes in path order; a repository can place files outside the checkout directory, and a repository file `.rill/checkout.toml` overwrites the client's endpoint and token state. | [16](16-security-hardening.md) |
| Path-scoped permissions are nominal: `GetTree`, `GetBlob` and the object routes need only repository-level read, so a reader of one directory can fetch any object by hash. `Grep` is the one read handler without a `permit().read`. | [05](05-authorization-audit.md) |
| Several writes are not atomic or not idempotent (`put_change_version`, `append_operation`, review submit), the land queue and pending auto-reverts live only in process memory, and no backend-independent recovery procedure exists. | [01](01-storage-durability.md), [02](02-crash-recovery-and-consistency.md) |
| No multi-node configuration exists: `open_stores` handles memory, local and S3-with-local-redb only; FoundationDB is unreachable from config, and derived data (search shards, import map) is written as single metadata values that would exceed FDB's 100 KB value limit. | [01](01-storage-durability.md), [11](11-search.md) |

## Briefs

### Trust the data

| # | Brief | Gap in one line |
| --- | --- | --- |
| 01 | [Storage durability](01-storage-durability.md) | S3 is tested only against in-memory and `file://` stores and assumes conditional put; FoundationDB has no tests and no server wiring; redb durability is never set; objects are not re-verified on read. |
| 02 | [Crash recovery and consistency](02-crash-recovery-and-consistency.md) | Writes span object store, metadata and follow-up records without one transaction; the single-lander assumption is unenforced; nothing survives a restart except what the stores hold. |
| 03 | [Backup, restore and migration](03-backup-restore-and-migration.md) | No export format, no schema version, no backend-to-backend copy, no garbage collection of unreachable objects. |

### Trust the caller

| # | Brief | Gap in one line |
| --- | --- | --- |
| 04 | [Authentication and identity](04-authentication-and-identity.md) | Static tokens or self-issued Biscuits; no identity provider, rotation, revocation, teams or audit log; `author` and signatures on a change are client-supplied and unverified. |
| 05 | [Authorization audit](05-authorization-audit.md) | Per-RPC permission table from the code; path scoping does not reach object reads; several write paths skip path checks; a repository with no tuples is open to every authenticated principal. |
| 16 | [Security hardening](16-security-hardening.md) | Threat model, no TLS, host-process execution, symlink and allocation issues in materialize, missing input limits, fuzz targets, dependency and release supply chain. |

### Run it

| # | Brief | Gap in one line |
| --- | --- | --- |
| 06 | [Operations and observability](06-operations-and-observability.md) | No metrics or trace export, health never changes, only Ctrl-C handled, drain has no deadline, no rate or size limits, no packaging or runbooks. |
| 07 | [Sandboxed execution](07-sandboxed-execution.md) | `LocalSandbox` is the only sandbox and isolates nothing; `FirecrackerSandbox` always returns `NotSupported`; no resource limits, secrets or artifact storage. |
| 15 | [Scale and performance](15-scale-and-performance.md) | Complexity of every hot path; the search indexer, memory backend, list RPCs and landing record are O(repo); no benchmarks or load tests exist. |
| 17 | [Code health and testing](17-code-health-and-testing.md) | Seven files over 400 lines with split plans, defect inventory from the documentation audit, no CI, no property, fuzz or fault-injection tests, no release process. |

### Make it adoptable

| # | Brief | Gap in one line |
| --- | --- | --- |
| 08 | [Web UI](08-web-ui.md) | None exists; the minimal review surface, browser authentication, and serving it from the same gRPC. |
| 09 | [Virtual filesystem](09-virtual-filesystem.md) | Only eager sparse checkout; lazy-fetch design, object protocol needs, status without a full walk. |
| 10 | [Structural merge](10-structural-merge.md) | A single line-based `merge_file_state`; what a syntax-aware merge needs and how to keep conflicts as data. |
| 11 | [Search](11-search.md) | In-memory postings rewritten as whole shards, per-root manifests that cost O(repo) per land, no ranking, symbols never served. |
| 12 | [Large files](12-large-files.md) | Chunking exists but every transport buffers whole files; no resumable upload, no tiering. |
| 13 | [Git interoperability](13-git-interop.md) | One-way offline import; no export, mirror or coexistence with GitHub. |
| 14 | [Notifications and webhooks](14-notifications-and-webhooks.md) | Event bus is in-process and lossy; no durable outbox, webhooks, or chat/email delivery. |

## Suggested order

1. **Safe to hold code**: 01, 02, 03, then TLS from 16. A deployment that
   can lose or leak a landing is not a source control system.
2. **Safe to share**: 04, 05, the sandbox from 07, the limits from 06 and 16.
   After this a team can be given tokens.
3. **Operable**: the rest of 06, 17 (CI first), 15 (benchmarks, then the
   O(repo) fixes in search and the landing record).
4. **Adoptable**: 08, 14, 13 together make a transition from GitHub
   possible; 09, 10, 11, 12 are each a project in their own right and can be
   sequenced by the first real repository's needs.

Items in 1 and 2 are weeks of focused work; each item in 4 is a quarter.
