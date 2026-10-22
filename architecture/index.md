# Rill architecture

Rill is source control for codebases written by humans and AI agents
together. One server owns each repository. Every person and agent works in a
private server-side workspace, submits changes for review, and the lander
puts them on a single numbered trunk (`trunk@42`). Objects are immutable and
content-addressed with BLAKE3, conflicts are stored as data instead of
aborting, and review, landing, verification, search and permissions are part
of the server. Rill is not git-compatible; a one-way importer migrates git
history.

These pages describe the code as it is. The source in `crates/` is the
authority: if a page disagrees with the code, the code wins and the page is
fixed. The non-negotiable rules are on
[principles and invariants](01-overview/principles-and-invariants.md).

## Reading order

Read in this order the first time; each page ends with Previous / Next links.

**01 Overview**

1. [System model](01-overview/system-model.md)
2. [Principles and invariants](01-overview/principles-and-invariants.md)
3. [Crates and layering](01-overview/crates-and-layering.md)
4. [Glossary](01-overview/glossary.md)

**02 Data**

5. [Objects and ids](02-data/objects-and-ids.md)
6. [Storage and keyspace](02-data/storage-and-keyspace.md)
7. [Metadata engines](02-data/metadata-engines.md)
8. [Merge algebra](02-data/merge-algebra.md)
9. [Content and chunking](02-data/content-and-chunking.md)
10. [Derived data](02-data/derived-data.md)

**03 Core algorithms**

11. [Diff](03-core-algorithms/diff.md)
12. [Merge](03-core-algorithms/merge.md)
13. [Rebase](03-core-algorithms/rebase.md)
14. [Trees](03-core-algorithms/trees.md)
15. [History graph](03-core-algorithms/history-graph.md)
16. [Change builder](03-core-algorithms/change-builder.md)

**04 Workflow**

17. [Workspaces](04-workflow/workspaces.md)
18. [Checkouts](04-workflow/checkouts.md)
19. [Changes and stacks](04-workflow/changes-and-stacks.md)
20. [Review](04-workflow/review.md)
21. [Review agents](04-workflow/review-agents.md)
22. [Landing](04-workflow/landing.md)
23. [Post-land verification](04-workflow/post-land-verification.md)
24. [Intents](04-workflow/intents.md)

**05 Policy**

25. [Identity and authentication](05-policy/identity-and-auth.md)
26. [Capability tokens](05-policy/capability-tokens.md)
27. [Token keys](05-policy/token-keys.md)
28. [Permissions](05-policy/permissions.md)
29. [Risk classification](05-policy/risk-classification.md)
30. [Submit policy](05-policy/submit-policy.md)
31. [Signing](05-policy/signing.md)
32. [Audit log](05-policy/audit.md)
33. [Data protection](05-policy/data-protection.md)
34. [Threat model](05-policy/threat-model.md)
35. [Security review packet](05-policy/security-review.md)

**06 Services**

36. [Search](06-services/search.md)
37. [Exec](06-services/exec.md)
38. [Remote execution](06-services/remote-execution.md)
39. [Git import and export](06-services/git-import.md)

**07 Runtime**

40. [Server](07-runtime/server.md)
41. [API surface](07-runtime/api-surface.md)
42. [Events and workers](07-runtime/events-and-workers.md)
43. [Configuration](07-runtime/configuration.md)
44. [Observability](07-runtime/observability.md)
45. [Cache tier](07-runtime/cache.md)

**08 Clients**

46. [SDK](08-clients/sdk.md)
47. [CLI](08-clients/cli.md)
48. [MCP server](08-clients/mcp.md)

**09 Engineering**

49. [Development](09-engineering/development.md)
50. [Testing](09-engineering/testing.md)
51. [Simulation](09-engineering/simulation.md)
52. [Performance](09-engineering/performance.md)

**10 Decisions**

53. [Architecture decisions](10-decisions/README.md): the sixteen ADRs that
    fix the target (v2) design. They are records, not descriptions of the
    current code, and are reached from that page.

## Where to look for X

| Question | Page |
| --- | --- |
| Which rules must never break | [Principles and invariants](01-overview/principles-and-invariants.md) |
| Which crate owns something, who may depend on whom | [Crates and layering](01-overview/crates-and-layering.md) |
| What a term means | [Glossary](01-overview/glossary.md) |
| How ids and hashes are computed, what a `ChangeVersion` holds | [Objects and ids](02-data/objects-and-ids.md) |
| Where a record is stored, which backends exist | [Storage and keyspace](02-data/storage-and-keyspace.md) |
| How memory, redb and FoundationDB implement the metadata store | [Metadata engines](02-data/metadata-engines.md) |
| How a conflict is represented | [Merge algebra](02-data/merge-algebra.md) |
| How large files are split | [Content and chunking](02-data/content-and-chunking.md) |
| Root trees, blame, path history, caching | [Derived data](02-data/derived-data.md) |
| Why a rebase never fails | [Rebase](03-core-algorithms/rebase.md) |
| Log, ancestry and merge-base queries | [History graph](03-core-algorithms/history-graph.md) |
| How a working copy reaches the server | [Workspaces](04-workflow/workspaces.md), [Checkouts](04-workflow/checkouts.md) |
| Amend, versions and stacks | [Changes and stacks](04-workflow/changes-and-stacks.md) |
| Labels, votes, threads | [Review](04-workflow/review.md) |
| How trunk moves and sequence numbers are assigned | [Landing](04-workflow/landing.md) |
| Bisect and automatic revert | [Post-land verification](04-workflow/post-land-verification.md) |
| Who a caller is and what they may do | [Identity and authentication](05-policy/identity-and-auth.md), [Capability tokens](05-policy/capability-tokens.md), [Token keys](05-policy/token-keys.md), [Permissions](05-policy/permissions.md) |
| What a change needs before it may land | [Risk classification](05-policy/risk-classification.md), [Submit policy](05-policy/submit-policy.md) |
| Who did what, provably | [Audit log](05-policy/audit.md) |
| Keeping secrets out, redacting what leaked | [Data protection](05-policy/data-protection.md) |
| Known threats and which phase closes each | [Threat model](05-policy/threat-model.md), [Security review packet](05-policy/security-review.md) |
| Code search and symbols | [Search](06-services/search.md) |
| Pre-land checks and the action cache | [Exec](06-services/exec.md) |
| REAPI, Bazel builds, workers and sandboxes | [Remote execution](06-services/remote-execution.md) |
| How the server is assembled and a request is handled | [Server](07-runtime/server.md) |
| Which RPCs exist | [API surface](07-runtime/api-surface.md) |
| Events, background workers | [Events and workers](07-runtime/events-and-workers.md) |
| Server flags, config file, dev mode | [Configuration](07-runtime/configuration.md) |
| Metrics, logs, traces, health endpoints, dashboards | [Observability](07-runtime/observability.md) |
| The regional cache in front of the origin | [Cache tier](07-runtime/cache.md) |
| Client library, `rill` commands, agent tools | [SDK](08-clients/sdk.md), [CLI](08-clients/cli.md), [MCP server](08-clients/mcp.md) |
| Building, CI, tests | [Development](09-engineering/development.md), [Testing](09-engineering/testing.md) |
| Seeded fault simulation, benchmarks, load | [Simulation](09-engineering/simulation.md), [Performance](09-engineering/performance.md) |
| Fixed design decisions for v2 | [Architecture decisions](10-decisions/README.md) |

Gaps before production and the v2 plan are kept as planning notes in the
local, untracked `research/` folder; they are not part of this knowledge base.

## Keeping this current

When behaviour changes, rewrite the page that owns it in the same change.
Every page starts with a short summary, then the essential mechanics, a
`Limitations` section stating present gaps as facts, a `Code` section linking
the main sources, and a navigation footer. When a page is added, moved or
removed, update the reading order above and the footers of its neighbours.

---
[Index](index.md) · Next: [System model](01-overview/system-model.md) →
