# Architecture

Technical knowledge base for Rill's current implementation. Start with the
[system model](system.md), then follow the boundary relevant to your work.
Source, protobuf definitions and tests remain authoritative.

## Data

- [Objects](data/objects.md): ids, hashing, blobs, trees, change versions, refs, records.
- [Merge algebra](data/merge-algebra.md): `Merge<T>`, conflicts carried as data.
- [Storage](data/storage.md): `ObjectStore`/`MetaStore`, keyspace, the four backends.
- [Derived data](data/derived-data.md): root trees, blame, history, summaries and invalidation.
- [Blobs](data/blobs.md): chunking and reassembly of large content.

## Core algorithms

- [Diff](core/diff.md): line and token diffs, hunks, unified output, tree diff.
- [Merge and rebase](core/merge-and-rebase.md): three-way merge, markers, rebase onto a new parent.
- [Trees](core/trees.md): building, applying changes, materializing to disk.
- [Graph](core/graph.md): ancestry, generation numbers, log and merge-base queries.
- [Builder](core/builder.md): forming a `ChangeVersion` from a snapshot.

## Workflow

- [Workspaces](workflow/workspaces.md): server-side state, checkout, status, sync.
- [Changes](workflow/changes.md): new, amend, versions, stacks.
- [Review](workflow/review.md): labels, votes, threads, interdiff, evaluation.
- [Landing](workflow/landing.md): queue, rebase onto trunk, CAS, sequence numbers.
- [Post-land](workflow/post-land.md): continuous verification, bisect, revert.
- [Intents](workflow/intents.md): what a change is for, similarity, policy facts.

## Policy

- [Identity and authentication](policy/identity-and-authentication.md): principals, tokens, dev mode.
- [Capability tokens](policy/capability-tokens.md): Biscuit issuance, attenuation, verification.
- [Permissions](policy/permissions.md): relationship tuples, directory inheritance, read filtering.
- [Submit policy](policy/submit-policy.md): risk classes and Cedar rules.
- [Signing](policy/signing.md): signed provenance on changes.

## Services

- [Search](services/search.md): trigram index, symbols, the indexer worker.
- [Exec](services/exec.md): action cache, sandboxes, verify plans.
- [Git import](services/git-import.md): one-way history import.

## Runtime and integrations

- [Server](runtime/server.md): composition, request lifecycle, object protocol, events, workers, config.
- [Protocol](runtime/protocol.md): the `rill.v1` surface and conversion rules.
- [SDK](integrations/sdk.md), [CLI](integrations/cli.md) and [MCP](integrations/mcp.md).

## Engineering

- [Development](engineering/development.md): workspace layout, layering, build profiles.
- [Testing](engineering/testing.md): test kinds, fixtures, what is covered.

## Keeping this useful

Describe what the code does now: ownership, contracts, state transitions,
transaction boundaries, and limitations that affect callers. Keep historical
changes, checklists and plans out of these pages; those belong in
[research](../research/README.md).

One page owns each topic. Link to related flows and source rather than
repeating their implementation. Every page carries at least one diagram of the
real mechanism, a Code section linking the files it describes, and a
Limitations section that states present gaps as facts.
