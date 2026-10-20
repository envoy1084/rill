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
7. [Merge algebra](02-data/merge-algebra.md)
8. [Content and chunking](02-data/content-and-chunking.md)
9. [Derived data](02-data/derived-data.md)

**03 Core algorithms**

10. [Diff](03-core-algorithms/diff.md)
11. [Merge](03-core-algorithms/merge.md)
12. [Rebase](03-core-algorithms/rebase.md)
13. [Trees](03-core-algorithms/trees.md)
14. [History graph](03-core-algorithms/history-graph.md)
15. [Change builder](03-core-algorithms/change-builder.md)

**04 Workflow**

16. [Workspaces](04-workflow/workspaces.md)
17. [Checkouts](04-workflow/checkouts.md)
18. [Changes and stacks](04-workflow/changes-and-stacks.md)
19. [Review](04-workflow/review.md)
20. [Review agents](04-workflow/review-agents.md)
21. [Landing](04-workflow/landing.md)
22. [Post-land verification](04-workflow/post-land-verification.md)
23. [Intents](04-workflow/intents.md)

**05 Policy**

24. [Identity and authentication](05-policy/identity-and-auth.md)
25. [Capability tokens](05-policy/capability-tokens.md)
26. [Permissions](05-policy/permissions.md)
27. [Risk classification](05-policy/risk-classification.md)
28. [Submit policy](05-policy/submit-policy.md)
29. [Signing](05-policy/signing.md)
30. [Threat model](05-policy/threat-model.md)

**06 Services**

31. [Search](06-services/search.md)
32. [Exec](06-services/exec.md)
33. [Git import](06-services/git-import.md)

**07 Runtime**

34. [Server](07-runtime/server.md)
35. [API surface](07-runtime/api-surface.md)
36. [Events and workers](07-runtime/events-and-workers.md)
37. [Configuration](07-runtime/configuration.md)

**08 Clients**

38. [SDK](08-clients/sdk.md)
39. [CLI](08-clients/cli.md)
40. [MCP server](08-clients/mcp.md)

**09 Engineering**

41. [Development](09-engineering/development.md)
42. [Testing](09-engineering/testing.md)
43. [Simulation](09-engineering/simulation.md)
44. [Performance](09-engineering/performance.md)

**10 Decisions**

45. [Architecture decisions](10-decisions/README.md): the twelve ADRs that
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
| Who a caller is and what they may do | [Identity and authentication](05-policy/identity-and-auth.md), [Capability tokens](05-policy/capability-tokens.md), [Permissions](05-policy/permissions.md) |
| What a change needs before it may land | [Risk classification](05-policy/risk-classification.md), [Submit policy](05-policy/submit-policy.md) |
| Known threats and which phase closes each | [Threat model](05-policy/threat-model.md) |
| Code search and symbols | [Search](06-services/search.md) |
| Pre-land checks and the action cache | [Exec](06-services/exec.md) |
| How the server is assembled and a request is handled | [Server](07-runtime/server.md) |
| Which RPCs exist | [API surface](07-runtime/api-surface.md) |
| Events, background workers | [Events and workers](07-runtime/events-and-workers.md) |
| Server flags, config file, dev mode | [Configuration](07-runtime/configuration.md) |
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
