# Permissions

Access to a repository is a set of `(object, relation, subject)` tuples stored
in the repository's metadata. Objects are the repository (`repo`) or
directories (`dir:/src/billing`). Subjects are a principal, a team, a
principal kind or everyone. `Authorizer` applies the relation order and
directory inheritance over a versioned snapshot. The invariants:

* a relation granted on a directory applies to its whole subtree;
* a store error denies rather than allowing;
* a check reflects at least every relation change its caller has seen.

## Model

```mermaid
flowchart LR
    O[owner] --> W[writer] --> RV[reviewer] --> R[reader]
```

Each relation implies the ones after it:
[`Relation::satisfied_by`](../../crates/domain/policy/src/rebac/model.rs)
accepts a tuple holding the relation itself or any stronger one. There are no
deny tuples, so checks are monotonic and snapshots cache safely.

Subjects as stored, parsed by `Subject::from_str`:

| Form | Matches |
| --- | --- |
| `principal:<PrincipalId>` | `id == principal.id` |
| `team:<name>` | `name` in `AuthContext::teams` (the user's SCIM or sign-in groups); non-empty, no `/` |
| `kind:human`, `kind:agent`, `kind:service` | `principal.kind` |
| `*` | everyone |

Clients may also name `user:<name>` and `agent:<name>`. These must exist in
the repository's org (outside dev mode) and are stored by principal id.

`dir_object(path)` is `repo` for the root and `dir:/<path>` otherwise.
`inheritance_chain(object)` lists the object, then each ancestor directory,
then `repo`.

## Storage

Tuples are keys with empty values in the repository's metadata
([`keys.rs`](../../crates/foundation/store/src/keys.rs)), written in one
transaction by
[`add_relation` and `remove_relation`](../../crates/foundation/store/src/repo/relations.rs):

| Key | Purpose |
| --- | --- |
| `(rebac, ns, repo, object, relation, subject)` | tuples by object |
| `(rebacs, ns, repo, subject, object, relation)` | the same tuples by subject (`relations_of_subject`) |
| `(rebacv, ns, repo)` | the relation version, a counter every change bumps |

Each change also writes a `RelationChanged` event. Both calls return the
relation version after the change.

## Snapshot and authorizer

[`Snapshot::load`](../../crates/domain/policy/src/rebac/snapshot.rs) reads the
version, then every tuple, so the tuples reflect at least that version. Tuples
that do not parse are logged and skipped. The snapshot indexes tuples by
object:

* `check` looks up only the objects on the inheritance chain;
* `grants_for(relation, object)` returns the satisfying tuples, nearest
  first, which is what `Check` reports as the reason;
* `granted_dirs` lists the directories where a principal directly holds a
  relation.

[`Authorizer`](../../crates/domain/policy/src/rebac/authorizer.rs) is one per
`RepoContext` and holds one snapshot in a
[`SnapshotCache`](../../crates/domain/policy/src/rebac/cache.rs). The held
snapshot is served while it is at least the requested version and younger
than `DEFAULT_SNAPSHOT_TTL` (30 s, a backstop). It is replaced when:

* a grant or revoke goes through this server's authorizer;
* this server's follower (`workers/follower.rs`) reads a `RelationChanged`
  event written by any server;
* a request asks for a newer version than the one held.

### Consistency across servers

Clients send the relation versions they have seen in the `x-rill-relations`
header: `rp_<hex>=<version>` pairs, separated by commas. `AccessService`
returns a version from every call, and the SDK records versions from grants
and revokes and sends them on every request. A server holding an older
snapshot reloads before checking, so a revoke is honoured on every server
from its author's next request, even before the event log reaches them. A
version the store has not reached is refused with `INVALID_ARGUMENT`.

## Read filtering

[`PathFilter`](../../crates/domain/policy/src/rebac/path_filter.rs) is a set of
path prefixes, built once per request. Relations, token paths and agent
scopes are all prefix sets, so `and` intersects them exactly.

* `allows(path)` covers a path and its subtree.
* `sees_into(dir)` also covers the directories on the way to an allowed
  one.

`Permit::read_filter()` is `allow_all` in dev mode or for an org
administrator. Otherwise it is the caller's `Reader` directories, narrowed by
the token's paths (a token for another repository, or without `read`, denies
all) and the agent's registered scope. `Permit::read_any()` is the same
filter, refused when it is empty.

A reader of one directory therefore sees a repository as just that
directory:

| Surface | Behaviour for a path-limited reader |
| --- | --- |
| `ListDir` | Lists any directory it `sees_into`. Entries are readable ones, plus directories on the way to them (with a blank summary, so hidden files are not counted). |
| `ResolvePath`, `PathHistory`, `Blame` | `read(path)` |
| `Diff`, `Grep` | Only readable paths. |
| `GetChangeVersion`, `GetChangeInfo`, `GetChangeVersions`, `Log`, `WatchLog` | A version that changes nothing readable is not found or skipped: its message and author would describe hidden work. Others show only readable changes, with `files_changed` counting those. |
| `GetRepo`, `ListRepos`, refs, `TrunkHead`, `TrunkRange`, `WatchTrunk`, `WatchRefs`, `MergeBase`, `IsAncestor` | `read_any`: hashes and names only |
| `GetTree`, `GetBlob`, object `GET`/`HEAD`/batch | A read grant (below) |
| `HasObjects` | Write access somewhere (`write_any`): existence answers only to an uploader |
| `WatchEvents`, `GetDerived`, `WaitDerived`, reviews, intents, lander status, policies, workspaces | Read on the repository root |

## Read grants

[Grants](../../crates/domain/policy/src/grants/mod.rs) close the gap that
content addressing opens: an object is named by its hash, and a hash says
nothing about the path it was reached through.

A response that reveals object hashes carries a signed `read_grant` for the
objects under paths the caller may read:

* `ListDir` grants file content and directories;
* `ResolvePath` grants the entry;
* `GetTree` grants every entry of the granted tree;
* `Diff` grants both sides of each file;
* `GetChangeVersion` grants the content of readable changes.

A grant (`ReadGrant`) lists `(kind, hash, path)` entries for one principal
and repository, and expires within ten minutes (sooner if the token does).
[`GrantBuilder`](../../crates/server/api/src/grants/issue.rs) adds a
directory's tree only when the caller may read the whole directory, because
a tree object names every child. A version object is added only for readers
of the whole repository.

Every fetch by hash requires a grant naming the object, for the caller and
repository ([`grants::require`](../../crates/server/api/src/grants/require.rs)):

* `GetBlob` and `GetTree` take it as `grant`;
* the object protocol takes `x-rill-grant` or `?grant=`;
* a chunk is fetched through a granted manifest that lists it (`?via=`).

Dev mode skips the check. `GetTree` continues the walk from the path in the
grant entry. The SDK records each response's grant for the hashes in it and
sends it when fetching them (`Client::read_grants`).

Grants are signed with ECDSA P-256 by a key derived from the active token
key (`GrantSigner::derive`), so grant keys are wrapped by the same key
service and rotate and retire with token keys. `TokenKeyRecord` stores the
grant public key. `GET /public-keys` publishes the public keys so a cache tier
can verify grants without the metadata store.

Content identical in a readable and a hidden path is readable through the
readable path; content is content. Across organizations, objects are never
shared ([ADR 014](../10-decisions/014-objects-per-organization.md)).

## Enforcement

[`Permit`](../../crates/server/api/src/auth/permit.rs) maps each operation to
a verb, a relation and a path:

| Method | Verb | Relation | Typical callers |
| --- | --- | --- | --- |
| `read(path)` | `Read` | `Reader` | every read RPC, `GetPolicy`, `EvaluateSubmit`, object `GET`/`HEAD` |
| `write(path)` | `Write` | `Writer` (`Owner` under `.rill/`) | every path a stored version writes, inside its commit (below) |
| `write_any()` | `Write` | `Writer` somewhere | `Rebase`, `Merge`, `SubmitForReview`, `SetStack`, `Abandon`, `PutBlob`, object `PUT`, `HasObjects` |
| `write(root)` | `Write` | `Writer` | workspace and intent changes, `MoveRef` |
| `review()` | `Review` | `Reviewer` | `Vote`, `Comment`, thread reply and resolve |
| `land()` | `Land` | `Writer` | `RequestLand`, `ReportVerification`, `RevertSeq`, `Bisect` |
| `admin()` | `Admin` | `Owner` | relation changes and listing, `Expand`, checks about others, `SetPolicy`, `IssueToken` without a token |

### Writes

Every stored version passes one check inside its commit
([`services/repo/authorize.rs`](../../crates/server/api/src/services/repo/authorize.rs)),
whichever RPC made it. `write_set` lists what the version writes:

* every file and subtree change;
* for a merge, only the paths it leaves differently from every other
  parent, since what it takes from the other side is brought in, not
  written.

The committer needs `writer` on each of those paths (`owner` under `.rill/`)
and `reader` on each copy source. The commit transaction asserts the
relation version the decision used, so a relation change racing the commit
makes it conflict and decide again. Handlers may refuse earlier, but the
commit decides.

Landing checks again. The lander's approval hook (`WriteRights`) refuses a
version whose author no longer holds those relations. The author's teams,
agent scope and org-admin role count here; their token does not.

A check passes the token's capability, then the agent's scope, then either
the tuples or org administration. Instance administrators, and users with the
org's `admin` role presenting an unnarrowed token for it
([`administers_org`](../../crates/server/api/src/auth/instance.rs)), are owners
of every repository in the org. Dev mode skips the third layer.

A repository with no tuples is closed to everyone but org administrators.
`CreateRepo` makes its creator `owner`, so the creator can grant access, and
an org administrator can recover a repository whose last owner left.

`owns_or_admin(owner)` passes when the caller is the record's owner, and
otherwise requires `admin()`.

## AccessService

[`AccessService`](../../crates/server/api/src/services/access/mod.rs) names the
repository (`org/name` or a bare name) and a directory path in each tuple.

| RPC | Needs | Does |
| --- | --- | --- |
| `AddRelation`, `RemoveRelation` | `admin()` | change a tuple; returns the repository id and relation version |
| `ListRelations` | `admin()` | tuples, filtered by path, subject or relation |
| `Check` | nothing about oneself; `admin()` about another `user:`/`agent:` | whether the relation is held, the granting tuples nearest first, and whether by org administration |
| `Expand` | `admin()` | every granting tuple on a path, with the org's active users in each team subject |
| `LookupPaths` | as `Check` | the outermost directories where a subject holds a relation (`""` for all) |

The CLI is `rill access grant/revoke/list/check/who/where`
([CLI](../08-clients/cli.md)). The SDK is `Client::access()`
([SDK](../08-clients/sdk.md)).

## Limitations

* `principal:<id>` subjects are not validated; only `user:` and `agent:`
  names are.
* There are no cross-repository roles beyond org administration.
* A snapshot reload reads every tuple of the repository. Changes are rare
  next to checks, but a repository with very many tuples pays for each
  reload.
* Grants list hashes exactly. A listing of a very large directory carries
  a large grant; compact grants for whole subtrees come with the VFS.
* Event streams, derived values, reviews, intents, lander status and
  workspaces need read on the whole repository rather than filtering for
  path-limited readers.

## Code

* [`rebac/model.rs`](../../crates/domain/policy/src/rebac/model.rs): `Relation`, `Subject`, `dir_object`, `inheritance_chain`.
* [`rebac/snapshot.rs`](../../crates/domain/policy/src/rebac/snapshot.rs): `Tuple`, `Snapshot`.
* [`rebac/cache.rs`](../../crates/domain/policy/src/rebac/cache.rs): `SnapshotCache`, `DEFAULT_SNAPSHOT_TTL`.
* [`rebac/authorizer.rs`](../../crates/domain/policy/src/rebac/authorizer.rs): `Authorizer`; [`rebac/path_filter.rs`](../../crates/domain/policy/src/rebac/path_filter.rs): `PathFilter`.
* [`rill-store/src/repo/relations.rs`](../../crates/foundation/store/src/repo/relations.rs): tuples, the subject index and the relation version.
* [`auth/permit.rs`](../../crates/server/api/src/auth/permit.rs): `Permit`; [`auth/instance.rs`](../../crates/server/api/src/auth/instance.rs): org administration.
* [`services/access/`](../../crates/server/api/src/services/access/mod.rs): `AccessService`.
* Tests: [`rebac/tests.rs`](../../crates/domain/policy/src/rebac/tests.rs), `rill-api/tests/grpc/access/` (the permission matrix, holes, grants, relations, writes).

---
← Previous: [Token keys](token-keys.md) · [Index](../index.md) · Next: [Risk classification](risk-classification.md) →
