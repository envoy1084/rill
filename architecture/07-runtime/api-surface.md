# API surface

The server exposes two wire surfaces. The first is the `rill.v1` gRPC
package, generated from protobuf in `rill-proto`. The second is a small HTTP
protocol for content-addressed objects that a cache or CDN can sit in front
of. `rill-proto` also holds the conversions between wire messages and
`rill-types`, so no crate below it depends on tonic or prost.

## Build

```mermaid
flowchart LR
    P[proto/rill/v1/*.proto] -->|protox| FDS[FileDescriptorSet]
    FDS -->|tonic-prost-build| Gen[rill::v1 messages, servers, clients]
    FDS -->|include_bytes| Refl[FILE_DESCRIPTOR_SET]
    T[rill-types] <-->|convert| Gen
    Gen --> S[rill-api]
    Gen --> C[rill-sdk]
```

`build.rs` compiles the nine `.proto` files with `protox`, so no `protoc`
binary is needed. It writes the descriptor set that
`rill_proto::FILE_DESCRIPTOR_SET` embeds for gRPC reflection. It configures
`.btree_map(".rill.v1")` and `.bytes(".rill.v1")`, so maps are ordered and
byte fields are `bytes::Bytes`, as in `rill-types`. It generates both server
and client stubs.

## gRPC services

The server registers the Rill services and the REAPI services behind `AuthLayer`
([server](server.md#request-path)) and `SessionService`, which signs callers
in, without it. It also registers `tonic-reflection` and
`tonic-health`, which report every service as serving.

| Service | Proto | Handler | Covers |
| --- | --- | --- | --- |
| `RepoService` | `repo.proto` | `services/repo/` | repositories, history, blame, grep, trees and blobs, diff, merge base, writes (`CreateChangeVersion`, `ApplyEdits`, `Rebase`, `Merge`, `Resolve`), refs and trunk, `CutRelease`, `Watch*` streams including `WatchEvents`, derived data |
| `WorkspaceService` | `workspace.proto` | `services/workspace/mod.rs` | server-side workspaces, snapshots, operation log, `Undo`, `WorkspaceDiff` |
| `ReviewService` | `review.proto` | `services/review/` | votes, threads, stacks, `EvaluateSubmit`, `RequestLand` and `RequestLandStack` (`onto` trunk or a release), `GetLandStatus`, `WatchReview` |
| `IntentService` | `intent.proto` | `services/intent.rs` | intents, change links, `FindSimilarIntents` |
| `LanderService` | `lander.proto` | `services/lander.rs`, `services/trunk_health.rs` | trunk status, landings, verification records and reports, `RevertSeq`, `Bisect`, `GetTrunkHealth`, `ListIncidents`, `FreezeTrunk` |
| `AdminService` | `admin.proto` | `services/admin/mod.rs`, `services/orgs.rs` | `WhoAmI`, `IssueToken`, `GetPolicy`/`SetPolicy`, `Health`, `CreateOrg`, `ListOrgs`, `AppendTrunk` (importers) |
| `AccessService` | `access.proto` | `services/access/` | `AddRelation`, `RemoveRelation`, `ListRelations`, `Check`, `Expand`, `LookupPaths` ([permissions](../05-policy/permissions.md#accessservice)) |
| `IdentityService` | `identity.proto` | `services/identity/` | `RevokeToken`, `RevokeSession`, `RotateTokenKeys`, `ListTokenKeys`, identity providers (`Put`/`List`/`DeleteIdentityProvider`), `ListUsers`, `UpdateUser`, agents (`RegisterAgent`, `UpdateAgent`, `ListAgents`), workload issuers (`Put`/`List`/`DeleteWorkloadIssuer`), `KillSwitch`, SCIM credentials ([identity and auth](../05-policy/identity-and-auth.md#the-identity-view)) |
| `AuditService` | `audit.proto` | `services/audit.rs` | `ListAuditEntries`, `ListAuditRoots`, `SealAudit`, `ListAuditKeys`, `WatchAudit` ([audit log](../05-policy/audit.md)) |
| `ProtectionService` | `protection.proto` | `services/protection.rs` | `RequestRedaction`, `ApproveRedaction`, `ListRedactions`, `ClearQuarantine`, `RotateDataKeys` ([data protection](../05-policy/data-protection.md)) |
| `ChecksService` | `checks.proto` | `services/checks/` | `ListRuns`, `GetRun`, `WatchRun` (stream), `ReadLog`, `ReadArtifact`, `ListNonhermetic`, `GetTestHistory`, `ListFlakyTests` (repository read access) ([checks](../06-services/checks.md)) |
| `SecretService` | `secrets.proto` | `services/secrets.rs` | `PutSecret`, `DeleteSecret`, `ListSecrets` (org administrators; values are write-only) ([exec](../06-services/exec.md#secrets)) |
| REAPI `Execution`, `ActionCache`, `ContentAddressableStorage`, `Capabilities`; `google.bytestream.ByteStream`; Buildbarn `OperationQueue` | `rill-reapi-proto` (vendored) | `services/exec/` | remote execution and the worker protocol ([remote execution](../06-services/remote-execution.md)) |
| `SessionService` | `identity.proto` | `services/session.rs` | `ListSignInProviders`, `ExchangeToken`, `RefreshSession`; no auth layer ([sign-in](../05-policy/identity-and-auth.md#sign-in)) |

`types.proto` holds the shared object model and `events.proto` the event
log's messages. The streaming RPCs are `GetBlob` (server stream of
`BlobChunk`), `PutBlob` (client stream: one `PutBlobHeader`, then `data`),
`WatchRefs`, `WatchTrunk`, `WatchLog` (a stream of `LogEntry { cursor,
info }`, resumable by cursor), `WatchReview`, and `WatchEvents`: every
committed state change as `EventEnvelope { cursor, Event }`, where `Event`
carries the actor, the time and a `kind` oneof with one message per event
kind. A `WatchEventsRequest` names kinds, a change id and a ref prefix to
filter by, and `after` (absent: now; empty: from the oldest retained event;
otherwise a cursor). [Events and workers](events-and-workers.md) describes how the
watches behave.

`LandStatus` on the wire is `QUEUED | REBASING | LANDED | FAILED | CANCELLED`.
`land_status_proto` folds `Verifying` and `Landing` into `REBASING`,
`Conflict` into `FAILED`, and `Returned` into `CANCELLED`. Admin requests
have no `repo` field. The repository is named inside the token scope or the
policy name (`<repo>/submit`, `<repo>/risk`); access tuples carry `repo` and
`path` fields.

## Message conventions

These conventions are stated at the top of `types.proto` and enforced by
`convert`:

* Object hashes are 32 raw bytes (BLAKE3). Record ids (`ChangeId`,
  `RepoId`, `OperationId`, ...) are 16 raw bytes. For an optional id, empty
  bytes mean absent.
* Paths are normalized `/`-separated strings, and `""` is the root.
  Timestamps are `int64` milliseconds since the Unix epoch, UTC.
* Every enum has `*_UNSPECIFIED = 0`. Decoding rejects zero and unknown
  values.
* `Merge<T>` terms travel as `MergeTerms.terms` in interleaved
  `[add0, remove0, ..., addN]` order. A list of even length is invalid (see
  [merge algebra](../02-data/merge-algebra.md)).
* `RootSpec` is a `oneof` of `ref`, `version_hash`, `tree_hash` and
  `trunk_seq`. Absent means the trunk head.
* Every repo-scoped request carries `string repo`: `org/name`, a bare name
  in the default org, `rp_<hex>` or bare hex. `Repo.org` names the org.
  `CreateOrg` takes 1-63 lowercase letters, digits and dashes; only humans
  may create orgs (anyone in dev mode). Creating `org/name` in a missing
  org is `NOT_FOUND`.
* Write requests carry `bytes operation_id`, and list requests carry
  `uint32 limit` (0 means the server default, clamped by `common::limit`).

## Retries

Every mutating RPC (35 of them; `PutBlob` is content-addressed and needs
none) runs through `services/idem.rs::once`. A request with a non-empty
`operation_id` claims `(principal, operation_id)` in the `idem` table
before it runs:

| Found | Result |
| --- | --- |
| nothing, or an expired claim | run; store the response when it succeeds, release the claim when it fails |
| the same request, finished | the stored response, without running again |
| the same request, still running | `UNAVAILABLE` ("already running"): retry |
| a different request (by BLAKE3 of its encoding) | `INVALID_ARGUMENT` |

Responses are kept 24 hours. A claim lasts 15 s (`CLAIM_TTL_MS`) and the
request renews it every 5 s while it runs, so the claim of a server that
died mid-request frees up within 15 s; a server task sweeps expired rows
every 10 minutes. The claim is visible to
the handler as a task-local `Scope`. The commit transaction records which
version the request created in the claim, so if the server dies after
committing and before storing the response, the retry takes over the
expired claim, finds that version and returns it instead of storing a
second one. A failed request whose claim recorded such an effect keeps the
claim (expired at once) for that retry.

The SDK retries every call that carries an operation id while the answer
is `UNAVAILABLE` (a dead or draining server, a cut connection, a claim a
dead server left), with backoff from 100 ms to 2 s for up to 20 s. With
three servers behind a load balancer, a server killed between committing a
write and answering it costs the client a retry that returns the first
outcome (`rill-api/tests/grpc/cluster/`).

## Conversion rules

Each module in `convert/` provides `From<&T>`/`From<T>` into the wire
message and `TryFrom<pb::T>` back to the `rill-types` value. The scalar
helpers live in `primitives.rs` (`hash_from_proto`, `id_from_proto` over the
`RillId` trait, `path_from_proto`, `enum_from_proto`, `required`, ...).
Decoding never guesses. A bad field yields a `ConvertError`: `BadLength`,
`Missing`, `BadEnum`, `Invalid` or `OutOfRange`. Each one names the field
with a dotted path such as `content_ref.blob.hash`. A `ConvertError` becomes
`INVALID_ARGUMENT` with that text. `convert/roundtrip_tests/` checks that
values survive a trip through the wire and back.

## HTTP object protocol

`objects/mod.rs` builds an axum router on the HTTP listener. The same
listener serves the sign-in routes (`/auth/*`, `/oauth/token`; see
[identity and auth](../05-policy/identity-and-auth.md#sign-in)) and SCIM
(`/scim/v2/<org>/…`; see
[provisioning](../05-policy/identity-and-auth.md#provisioning-scim-20)). An object URL
names exactly one byte string forever, so responses carry
`ETag: "<hash>"` and `Cache-Control: private, immutable, max-age=31536000`.
They are `private` because each fetch needs a read grant: a shared cache that
does not check grants must not serve them. The Rill cache tier checks grants
itself ([read grants](../05-policy/permissions.md#read-grants)).

| Route | Permission | Behaviour |
| --- | --- | --- |
| `GET`/`HEAD /o/{repo}/{kind}/{hash}` | a read grant naming the object (`x-rill-grant` header or `?grant=`); a chunk is fetched with `?via=<manifest>` and a grant naming the manifest; `derived` needs read on root | raw bytes as `application/octet-stream`; 403 without a covering grant, 404 when absent, 451 when redacted |
| `PUT /o/{repo}/{kind}/{hash}` | write on root | `rill_store::object::verify` checks the body hashes to `{hash}` (400 otherwise); 201 when new, 200 when it existed; body limit `MAX_PUT_BYTES` (256 MiB) |
| `POST /o/{repo}/batch` | read on something, and a grant in `x-rill-grant` | JSON `[{"kind","hash","via"?}]`, at most `MAX_BATCH` (10 000), streamed back as `application/vnd.rill.objects`; objects the grant does not name come back missing |
| `GET /public-keys` | none | the keys derived from each token key, as JSON `{"keys":[{"id","grant_public_key","audit_public_key","not_after"}]}`: grants verify with the first, audit roots with the second |
| `GET /redactions` | a service principal's unnarrowed token, or an instance administrator | `{"hashes":[…]}`: every redacted object, for cache tiers; 403 to anyone else, since a small file's hash confirms a guess at it |
| `GET /healthz` | none | `ok` |
| `GET /readyz` | none | 200 when every `health_components` entry is ok, else 503 listing the failures |

`{kind}` is one of `blob`, `chunk`, `manifest`, `tree`, `cv` or `derived`.
The batch response has one frame per requested object, in request order:

```text
kind u8 | hash [u8; 32] | length u64 BE | bytes
```

A missing object has `length = u64::MAX` and no bytes. Frames are produced
lazily, so a batch is never fully in memory. `require_auth` (`auth/http.rs`)
mirrors `AuthLayer` and answers 401. `ObjectError` maps server error codes
to HTTP statuses: 404, 403, 401, 400 (invalid argument or failed
precondition), 409 (already exists or aborted), 503, and 500 for the rest.

## Input limits

Every input is bounded. Exceeding a gRPC limit answers `OUT_OF_RANGE`
(message size) or `INVALID_ARGUMENT`; exceeding an HTTP limit answers 413.

| Limit | Value | Where |
| --- | --- | --- |
| gRPC request message | 16 MiB | `serve.rs` `MAX_REQUEST_BYTES`, every service |
| gRPC response message | 64 MiB | `MAX_RESPONSE_BYTES` |
| Requests in flight per connection | 1024 | `MAX_CONCURRENT_PER_CONNECTION` |
| Paths changed by one version | 100,000 | `limits.rs` `MAX_FILES_PER_CHANGE`; also edits per `ApplyEdits`, checked before any work |
| Encoded version | 8 MiB | `MAX_CHANGE_BYTES` |
| Version message | 64 KiB | `MAX_MESSAGE_BYTES` |
| File | 5 GiB | `MAX_FILE_BYTES`; `PutBlob` streams it |
| Inline content | 256 bytes | `INLINE_THRESHOLD`: a raw version over it is refused; `ApplyEdits` and `Resolve` store larger inline content as a blob |
| Path, component | 4096, 255 bytes | `MAX_PATH_BYTES`, `MAX_COMPONENT_BYTES` |
| Conflict terms, extra fields | 63, 32 (64-byte keys, 4 KiB values) | `MAX_MERGE_TERMS`, `MAX_EXTRA_*` |
| Object `PUT` body | 256 MiB | `objects/mod.rs` `MAX_PUT_BYTES` |
| Object batch | 10,000 | `MAX_BATCH` |
| SCIM body | 4 MiB | `scim/mod.rs` `MAX_SCIM_BODY` |
| Other HTTP bodies | 2 MiB | axum's default |
| Pages and results | log 10,000, grep 5,000, lander lists and workspace operations 1,000, diff 4 MiB and 1,000 files | each handler's `limit` |

### Rate limits

Every authenticated request, gRPC or object HTTP, is counted against its
principal's token bucket (`auth/rate.rs`, a keyed GCRA limiter from
`governor`): `[rate_limit] per_principal_per_sec` (1000; 0 turns it off)
refilling a bucket of `burst` (2000). Past it, gRPC answers
`RESOURCE_EXHAUSTED` and HTTP 429, both with `retry-after` in seconds.
Buckets are per server and idle ones are dropped every minute. A
registered agent's `requests_per_minute` budget is a second bucket of that
size refilled over a minute ([identity](../05-policy/identity-and-auth.md)).

## Limitations

* `SessionService` (sign-in) is not rate limited; nothing limits by
  client address.
* `next_page_token` exists on list responses, but the server always returns
  `None`. `limit` truncates and there is no continuation.
* A request without an `operation_id` is not deduplicated. Only the
  commit path records its effect in the claim; other handlers that fail
  after writing can be redone by a retry.
* `MergeBaseResponse.bases` is repeated, but the server returns at most one
  base.
* `rill.v1` is the only version. A missing RPC surfaces as `UNIMPLEMENTED`.
  The only client that handles this is the `Grep` fallback in `rill-mcp`.
* A batch read error is reported as a missing frame, not as an error.
* `PUT` stores any object kind after hash verification only. It does not run
  the existence checks of `commit_version`.

## Code

* [proto/rill/v1/types.proto](../../crates/foundation/proto/proto/rill/v1/types.proto), [repo.proto](../../crates/foundation/proto/proto/rill/v1/repo.proto), [workspace.proto](../../crates/foundation/proto/proto/rill/v1/workspace.proto), [review.proto](../../crates/foundation/proto/proto/rill/v1/review.proto), [intent.proto](../../crates/foundation/proto/proto/rill/v1/intent.proto), [lander.proto](../../crates/foundation/proto/proto/rill/v1/lander.proto), [admin.proto](../../crates/foundation/proto/proto/rill/v1/admin.proto)
* [build.rs](../../crates/foundation/proto/build.rs), [src/lib.rs](../../crates/foundation/proto/src/lib.rs), [src/error.rs](../../crates/foundation/proto/src/error.rs)
* [convert/mod.rs](../../crates/foundation/proto/src/convert/mod.rs), [convert/primitives.rs](../../crates/foundation/proto/src/convert/primitives.rs), [convert/roundtrip_tests/](../../crates/foundation/proto/src/convert/roundtrip_tests/mod.rs)
* [rill-api/src/serve.rs](../../crates/server/api/src/serve.rs), [services/mod.rs](../../crates/server/api/src/services/mod.rs)
* [objects/mod.rs](../../crates/server/api/src/objects/mod.rs), [objects/handlers.rs](../../crates/server/api/src/objects/handlers.rs), [objects/batch.rs](../../crates/server/api/src/objects/batch.rs), [auth/http.rs](../../crates/server/api/src/auth/http.rs)

---
← Previous: [Server](server.md) · [Index](../index.md) · Next: [Events and workers](events-and-workers.md) →
