# API reference

Generated from the `rill.v1` protos by `buf generate` with
`protoc-gen-doc`; do not edit by hand. Conventions (resource names,
revisions, pages, filters, etags, request ids, errors) are in
[API surface](api-surface.md).

- [AccessService](#rill-v1-AccessService)
- [AdminService](#rill-v1-AdminService)
- [AttentionService](#rill-v1-AttentionService)
- [AuditService](#rill-v1-AuditService)
- [ChangeService](#rill-v1-ChangeService)
- [ChecksService](#rill-v1-ChecksService)
- [ContentService](#rill-v1-ContentService)
- [EventService](#rill-v1-EventService)
- [IdentityService](#rill-v1-IdentityService)
- [IntentService](#rill-v1-IntentService)
- [LandingService](#rill-v1-LandingService)
- [NotificationService](#rill-v1-NotificationService)
- [OperationService](#rill-v1-OperationService)
- [OrgService](#rill-v1-OrgService)
- [PolicyService](#rill-v1-PolicyService)
- [ProtectionService](#rill-v1-ProtectionService)
- [RepoService](#rill-v1-RepoService)
- [ReviewService](#rill-v1-ReviewService)
- [ReviewerService](#rill-v1-ReviewerService)
- [SecretService](#rill-v1-SecretService)
- [SessionService](#rill-v1-SessionService)
- [TrunkService](#rill-v1-TrunkService)
- [WorkspaceService](#rill-v1-WorkspaceService)


## rill/v1/types.proto

Core object model of Rill, mirroring the `rill-types` crate.

Conventions:
  * object hashes are 32-byte BLAKE3 digests carried as `bytes`;
  * record ids (ChangeId, IntentId, ...) are 16 random bytes;
  * repository paths are normalized `/`-separated strings, "" is the root;
  * timestamps are int64 milliseconds since the Unix epoch, UTC;
  * `Merge<T>` terms are a repeated interleaved list
    [add0, remove0, add1, remove1, ..., addN] (always odd length).


<a name="rill-v1-BlobRef"></a>

### BlobRef

A stored blob or blob manifest with its total size.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `hash` | [bytes](#bytes) |  |  |
| `size` | [uint64](#uint64) |  |  |


<a name="rill-v1-ChangeInfo"></a>

### ChangeInfo

Metadata-only projection of a change version.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/versions/{hash}`. Output only. |
| `hash` | [bytes](#bytes) |  |  |
| `change_id` | [bytes](#bytes) |  |  |
| `parents` | [bytes](#bytes) | repeated |  |
| `predecessor` | [bytes](#bytes) | optional |  |
| `author` | [Principal](#rill-v1-Principal) |  |  |
| `provenance` | [Provenance](#rill-v1-Provenance) |  |  |
| `intent` | [bytes](#bytes) | optional |  |
| `message` | [string](#string) |  |  |
| `created_at` | [int64](#int64) |  |  |
| `files_changed` | [uint64](#uint64) |  |  |
| `has_conflicts` | [bool](#bool) |  |  |
| `trunk_seq` | [uint64](#uint64) | optional |  |


<a name="rill-v1-ChangeVersion"></a>

### ChangeVersion

One concrete version of a change; its hash is the version identity.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change_id` | [bytes](#bytes) |  |  |
| `parents` | [bytes](#bytes) | repeated |  |
| `predecessor` | [bytes](#bytes) | optional |  |
| `subtree_changes` | [ChangeVersion.SubtreeChangesEntry](#rill-v1-ChangeVersion-SubtreeChangesEntry) | repeated |  |
| `file_changes` | [ChangeVersion.FileChangesEntry](#rill-v1-ChangeVersion-FileChangesEntry) | repeated |  |
| `author` | [Principal](#rill-v1-Principal) |  |  |
| `provenance` | [Provenance](#rill-v1-Provenance) |  |  |
| `intent` | [bytes](#bytes) | optional |  |
| `message` | [string](#string) |  |  |
| `created_at` | [int64](#int64) |  |  |
| `extra` | [ChangeVersion.ExtraEntry](#rill-v1-ChangeVersion-ExtraEntry) | repeated |  |


<a name="rill-v1-ChangeVersion-ExtraEntry"></a>

### ChangeVersion.ExtraEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key` | [string](#string) |  |  |
| `value` | [bytes](#bytes) |  |  |


<a name="rill-v1-ChangeVersion-FileChangesEntry"></a>

### ChangeVersion.FileChangesEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key` | [string](#string) |  |  |
| `value` | [FileChange](#rill-v1-FileChange) |  |  |


<a name="rill-v1-ChangeVersion-SubtreeChangesEntry"></a>

### ChangeVersion.SubtreeChangesEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key` | [string](#string) |  |  |
| `value` | [SubtreeChange](#rill-v1-SubtreeChange) |  |  |


<a name="rill-v1-Comment"></a>

### Comment



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `author` | [Principal](#rill-v1-Principal) |  |  |
| `body` | [string](#string) |  |  |
| `created_at` | [int64](#int64) |  |  |
| `category` | [CommentCategory](#rill-v1-CommentCategory) | optional |  |
| `confidence` | [uint32](#uint32) | optional | Reviewer confidence as a percentage 0-100. |
| `suggestion` | [string](#string) | optional | Replacement text for the anchored lines, which the change's author can apply. |


<a name="rill-v1-ContentRef"></a>

### ContentRef

Reference to file content.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `inline` | [bytes](#bytes) |  |  |
| `blob` | [BlobRef](#rill-v1-BlobRef) |  |  |
| `chunked` | [BlobRef](#rill-v1-BlobRef) |  |  |


<a name="rill-v1-CopySource"></a>

### CopySource

Explicit copy source of a file, pinned to a parent version.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `path` | [string](#string) |  |  |
| `version` | [bytes](#bytes) |  |  |


<a name="rill-v1-DiffHunk"></a>

### DiffHunk

A contiguous block of changed lines.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `before_start` | [uint32](#uint32) |  |  |
| `before_len` | [uint32](#uint32) |  |  |
| `after_start` | [uint32](#uint32) |  |  |
| `after_len` | [uint32](#uint32) |  |  |
| `lines` | [DiffLine](#rill-v1-DiffLine) | repeated |  |


<a name="rill-v1-DiffLine"></a>

### DiffLine

One line of a structured diff.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `op` | [DiffLine.Op](#rill-v1-DiffLine-Op) |  |  |
| `text` | [bytes](#bytes) |  |  |


<a name="rill-v1-DirSummary"></a>

### DirSummary

Aggregate facts about a subtree.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `file_count` | [uint64](#uint64) |  |  |
| `dir_count` | [uint64](#uint64) |  |  |
| `total_size` | [uint64](#uint64) |  |  |
| `max_depth` | [uint32](#uint32) |  | Levels of directories below this one; 0 when it holds only files. |


<a name="rill-v1-FileChange"></a>

### FileChange

What happened to one path, relative to the first parent.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `set` | [FileChange.Set](#rill-v1-FileChange-Set) |  |  |
| `delete` | [FileChange.Delete](#rill-v1-FileChange-Delete) |  |  |
| `conflict` | [MergeTerms](#rill-v1-MergeTerms) |  |  |


<a name="rill-v1-FileChange-Delete"></a>

### FileChange.Delete




<a name="rill-v1-FileChange-Set"></a>

### FileChange.Set



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `content` | [ContentRef](#rill-v1-ContentRef) |  |  |
| `kind` | [FileKind](#rill-v1-FileKind) |  |  |
| `copy_from` | [CopySource](#rill-v1-CopySource) | optional |  |


<a name="rill-v1-FileDiff"></a>

### FileDiff

The difference of one path between two roots.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `path` | [string](#string) |  |  |
| `before` | [FileState](#rill-v1-FileState) |  |  |
| `after` | [FileState](#rill-v1-FileState) |  |  |
| `hunks` | [DiffHunk](#rill-v1-DiffHunk) | repeated | Structured hunks when the format is STRUCTURED. |
| `unified` | [string](#string) | optional | Unified diff text when the format is UNIFIED. |
| `binary` | [bool](#bool) |  |  |
| `conflict` | [MergeTerms](#rill-v1-MergeTerms) | optional | Set when `after` is a conflict rather than a resolved state. |


<a name="rill-v1-FileState"></a>

### FileState

A file's state on one side of a merge; no `content` means absent.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `content` | [ContentRef](#rill-v1-ContentRef) | optional |  |
| `kind` | [FileKind](#rill-v1-FileKind) |  |  |


<a name="rill-v1-Intent"></a>

### Intent

A task, spec, bug or plan that changes link to.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/intents/{intent}`. Output only. |
| `etag` | [string](#string) |  | Output only. |
| `id` | [bytes](#bytes) |  |  |
| `title` | [string](#string) |  |  |
| `body` | [string](#string) |  |  |
| `kind` | [IntentKind](#rill-v1-IntentKind) |  |  |
| `status` | [IntentStatus](#rill-v1-IntentStatus) |  |  |
| `owner` | [Principal](#rill-v1-Principal) |  |  |
| `parent` | [bytes](#bytes) | optional |  |
| `risk_override` | [RiskClass](#rill-v1-RiskClass) | optional |  |
| `changes` | [bytes](#bytes) | repeated |  |
| `created_at` | [int64](#int64) |  |  |
| `updated_at` | [int64](#int64) |  |  |
| `scope` | [IntentScope](#rill-v1-IntentScope) |  | What changes under the intent may do once approved; unset for none. |
| `revision` | [uint32](#uint32) |  | Bumped whenever the scope changes; approvals are of one revision. |
| `approvals` | [IntentApproval](#rill-v1-IntentApproval) | repeated |  |


<a name="rill-v1-IntentApproval"></a>

### IntentApproval

One approval of an intent's scope.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `by` | [Principal](#rill-v1-Principal) |  |  |
| `at` | [int64](#int64) |  |  |
| `revision` | [uint32](#uint32) |  | The scope revision approved. |
| `owns` | [string](#string) | repeated | The scope directories the approver owns. |


<a name="rill-v1-IntentScope"></a>

### IntentScope

The bounds of an approved intent: changes inside them inherit its
approval, anything outside falls back to normal review.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `paths` | [string](#string) | repeated | Globs every touched path must match. |
| `forbid_paths` | [string](#string) | repeated | Globs no touched path may match. |
| `max_files_per_change` | [uint32](#uint32) | optional |  |
| `max_changes` | [uint32](#uint32) | optional | Changes linked to the intent, at most. |
| `expires` | [int64](#int64) | optional |  |
| `agent_quorum` | [uint32](#uint32) | optional | Independent agent approvals each change needs. |
| `human_sample_permille` | [uint32](#uint32) | optional | Share of landed changes sampled for human audit, in thousandths. |
| `changes_per_day` | [uint32](#uint32) | optional | Changes linked per UTC day, at most. |
| `lands_per_day` | [uint32](#uint32) | optional | Changes landed per UTC day, at most. |


<a name="rill-v1-Label"></a>

### Label

A typed vote range on a change.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  |  |
| `min` | [int32](#int32) |  |  |
| `max` | [int32](#int32) |  |  |
| `copy_on_trivial_rebase` | [bool](#bool) |  |  |
| `max_requires_human` | [bool](#bool) |  |  |


<a name="rill-v1-LabelVote"></a>

### LabelVote

One principal's vote on one label for one version.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `label` | [string](#string) |  |  |
| `value` | [int32](#int32) |  |  |
| `voter` | [Principal](#rill-v1-Principal) |  |  |
| `version` | [bytes](#bytes) |  |  |
| `voted_at` | [int64](#int64) |  |  |
| `reason` | [string](#string) | optional |  |


<a name="rill-v1-Landing"></a>

### Landing

One landing: the version at a trunk position.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/trunk/positions/{N}`. |
| `position` | [uint64](#uint64) |  |  |
| `version` | [string](#string) |  | `orgs/{org}/repos/{repo}/versions/{hash}`. |
| `version_hash` | [bytes](#bytes) |  |  |
| `change_id` | [bytes](#bytes) |  |  |
| `landed_by` | [Principal](#rill-v1-Principal) |  |  |
| `landed_at` | [int64](#int64) |  |  |
| `reverts_position` | [uint64](#uint64) | optional | Set when this landing reverts an earlier position. |


<a name="rill-v1-MergeTerms"></a>

### MergeTerms

Interleaved `Merge<FileState>` terms; see the file header.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `terms` | [FileState](#rill-v1-FileState) | repeated |  |


<a name="rill-v1-NamedTreeEntry"></a>

### NamedTreeEntry

A tree entry together with its name, for listings.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  |  |
| `entry` | [TreeEntry](#rill-v1-TreeEntry) |  |  |


<a name="rill-v1-ObjectHash"></a>

### ObjectHash

A 256-bit BLAKE3 content hash wrapped as a message, for use where a
bare `bytes` field would be ambiguous (e.g. in `oneof`s and lists).

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `bytes` | [bytes](#bytes) |  |  |


<a name="rill-v1-OperationKind"></a>

### OperationKind



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `create` | [OperationKind.Create](#rill-v1-OperationKind-Create) |  |  |
| `snapshot` | [OperationKind.Snapshot](#rill-v1-OperationKind-Snapshot) |  |  |
| `new_change` | [OperationKind.NewChange](#rill-v1-OperationKind-NewChange) |  |  |
| `describe` | [OperationKind.Describe](#rill-v1-OperationKind-Describe) |  |  |
| `rebase` | [OperationKind.Rebase](#rill-v1-OperationKind-Rebase) |  |  |
| `resolve` | [OperationKind.Resolve](#rill-v1-OperationKind-Resolve) |  |  |
| `set_sparse` | [OperationKind.SetSparse](#rill-v1-OperationKind-SetSparse) |  |  |
| `handoff` | [OperationKind.Handoff](#rill-v1-OperationKind-Handoff) |  |  |
| `undo` | [OperationKind.Undo](#rill-v1-OperationKind-Undo) |  |  |
| `other` | [OperationKind.Other](#rill-v1-OperationKind-Other) |  |  |


<a name="rill-v1-OperationKind-Create"></a>

### OperationKind.Create




<a name="rill-v1-OperationKind-Describe"></a>

### OperationKind.Describe



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `version` | [bytes](#bytes) |  |  |


<a name="rill-v1-OperationKind-Handoff"></a>

### OperationKind.Handoff



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `to` | [Principal](#rill-v1-Principal) |  |  |


<a name="rill-v1-OperationKind-NewChange"></a>

### OperationKind.NewChange



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [bytes](#bytes) |  |  |
| `base` | [bytes](#bytes) |  |  |


<a name="rill-v1-OperationKind-Other"></a>

### OperationKind.Other



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `description` | [string](#string) |  |  |


<a name="rill-v1-OperationKind-Rebase"></a>

### OperationKind.Rebase



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `from` | [bytes](#bytes) |  |  |
| `to` | [bytes](#bytes) |  |  |


<a name="rill-v1-OperationKind-Resolve"></a>

### OperationKind.Resolve



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `version` | [bytes](#bytes) |  |  |


<a name="rill-v1-OperationKind-SetSparse"></a>

### OperationKind.SetSparse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `profile` | [SparseProfile](#rill-v1-SparseProfile) |  |  |


<a name="rill-v1-OperationKind-Snapshot"></a>

### OperationKind.Snapshot



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `tree` | [bytes](#bytes) |  |  |


<a name="rill-v1-OperationKind-Undo"></a>

### OperationKind.Undo



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `undone` | [bytes](#bytes) |  |  |


<a name="rill-v1-Org"></a>

### Org



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}`. |
| `org_id` | [string](#string) |  | The org's id, `{org}`. |
| `description` | [string](#string) | optional |  |
| `created_at` | [int64](#int64) |  |  |


<a name="rill-v1-Principal"></a>

### Principal

An authenticated actor: a human, an agent workload, or a service.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `id` | [bytes](#bytes) |  |  |
| `kind` | [PrincipalKind](#rill-v1-PrincipalKind) |  |  |
| `name` | [string](#string) |  |  |
| `identity` | [string](#string) | optional |  |


<a name="rill-v1-Provenance"></a>

### Provenance

Who and what produced a change.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `tool` | [string](#string) | optional |  |
| `model` | [string](#string) | optional |  |
| `session` | [string](#string) | optional |  |
| `prompt_ref` | [string](#string) | optional |  |
| `sponsor` | [Principal](#rill-v1-Principal) | optional |  |
| `resolved_by` | [string](#string) | optional |  |
| `extra` | [Provenance.ExtraEntry](#rill-v1-Provenance-ExtraEntry) | repeated |  |


<a name="rill-v1-Provenance-ExtraEntry"></a>

### Provenance.ExtraEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key` | [string](#string) |  |  |
| `value` | [string](#string) |  |  |


<a name="rill-v1-Ref"></a>

### Ref

A named pointer and its current target.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/refs/{ref}`. |
| `target` | [bytes](#bytes) |  |  |
| `trunk_position` | [uint64](#uint64) | optional | Set for `trunk`: the position of the head. |
| `etag` | [string](#string) |  | Changes whenever the target does; UpdateRef and DeleteRef take it. Output only. |


<a name="rill-v1-Repo"></a>

### Repo

A repository.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Output only. |
| `uid` | [bytes](#bytes) |  | The `rp_` id, which stays the same if the repository is renamed. Output only. |
| `repo_id` | [string](#string) |  | The repository's name within its org. Output only. |
| `description` | [string](#string) | optional |  |
| `created_at` | [int64](#int64) |  | Output only. |
| `trunk_head_position` | [uint64](#uint64) |  | Output only. |
| `windows_safe` | [bool](#bool) |  | Paths Windows cannot create are rejected. Immutable. |
| `org` | [string](#string) |  | The org the repository belongs to. Output only. |


<a name="rill-v1-ReviewState"></a>

### ReviewState

The review state of a change, keyed by its stable change id.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/changes/{change}/review`. Output only. |
| `change_id` | [bytes](#bytes) |  |  |
| `versions` | [bytes](#bytes) | repeated |  |
| `status` | [ReviewStatus](#rill-v1-ReviewStatus) |  |  |
| `intent` | [bytes](#bytes) | optional |  |
| `owner` | [Principal](#rill-v1-Principal) |  |  |
| `votes` | [LabelVote](#rill-v1-LabelVote) | repeated |  |
| `threads` | [Thread](#rill-v1-Thread) | repeated |  |
| `risk` | [RiskClass](#rill-v1-RiskClass) |  |  |
| `risk_reasons` | [string](#string) | repeated |  |
| `stack` | [bytes](#bytes) | repeated |  |
| `landed_seq` | [uint64](#uint64) | optional |  |
| `created_at` | [int64](#int64) |  |  |
| `updated_at` | [int64](#int64) |  |  |
| `extra` | [ReviewState.ExtraEntry](#rill-v1-ReviewState-ExtraEntry) | repeated |  |


<a name="rill-v1-ReviewState-ExtraEntry"></a>

### ReviewState.ExtraEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key` | [string](#string) |  |  |
| `value` | [string](#string) |  |  |


<a name="rill-v1-Signature"></a>

### Signature

A detached Ed25519 signature over a change version's hash.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `signer` | [Principal](#rill-v1-Principal) |  |  |
| `public_key` | [bytes](#bytes) |  |  |
| `signature` | [bytes](#bytes) |  |  |
| `signed_at` | [int64](#int64) |  |  |


<a name="rill-v1-SparseProfile"></a>

### SparseProfile

Which directories a workspace materializes.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `include` | [string](#string) | repeated |  |
| `exclude` | [string](#string) | repeated |  |


<a name="rill-v1-SubtreeChange"></a>

### SubtreeChange

A whole-directory operation, applied before file changes.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `copy` | [SubtreeChange.Copy](#rill-v1-SubtreeChange-Copy) |  |  |
| `delete` | [SubtreeChange.Delete](#rill-v1-SubtreeChange-Delete) |  |  |


<a name="rill-v1-SubtreeChange-Copy"></a>

### SubtreeChange.Copy



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `from_path` | [string](#string) |  |  |
| `from_version` | [bytes](#bytes) |  |  |


<a name="rill-v1-SubtreeChange-Delete"></a>

### SubtreeChange.Delete




<a name="rill-v1-Thread"></a>

### Thread



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/changes/{change}/review/threads/{thread}`. Output only. |
| `id` | [string](#string) |  |  |
| `anchor` | [ThreadAnchor](#rill-v1-ThreadAnchor) |  |  |
| `comments` | [Comment](#rill-v1-Comment) | repeated |  |
| `resolved` | [bool](#bool) |  |  |
| `to` | [Principal](#rill-v1-Principal) | optional |  |
| `outdated` | [bool](#bool) |  | The anchor was not found on the current version. |


<a name="rill-v1-ThreadAnchor"></a>

### ThreadAnchor

Where a thread is anchored.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [ThreadAnchor.Change](#rill-v1-ThreadAnchor-Change) |  |  |
| `lines` | [ThreadAnchor.Lines](#rill-v1-ThreadAnchor-Lines) |  |  |
| `file` | [ThreadAnchor.File](#rill-v1-ThreadAnchor-File) |  |  |


<a name="rill-v1-ThreadAnchor-Change"></a>

### ThreadAnchor.Change




<a name="rill-v1-ThreadAnchor-File"></a>

### ThreadAnchor.File



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `version` | [bytes](#bytes) |  |  |
| `path` | [string](#string) |  |  |


<a name="rill-v1-ThreadAnchor-Lines"></a>

### ThreadAnchor.Lines



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `version` | [bytes](#bytes) |  |  |
| `path` | [string](#string) |  |  |
| `start` | [uint32](#uint32) |  |  |
| `end` | [uint32](#uint32) |  |  |
| `context_hash` | [string](#string) |  |  |


<a name="rill-v1-Tree"></a>

### Tree

A directory: its entries keyed by name.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `entries` | [Tree.EntriesEntry](#rill-v1-Tree-EntriesEntry) | repeated |  |


<a name="rill-v1-Tree-EntriesEntry"></a>

### Tree.EntriesEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key` | [string](#string) |  |  |
| `value` | [TreeEntry](#rill-v1-TreeEntry) |  |  |


<a name="rill-v1-TreeEntry"></a>

### TreeEntry

What a directory entry points at.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `file` | [TreeEntry.File](#rill-v1-TreeEntry-File) |  |  |
| `dir` | [TreeEntry.Dir](#rill-v1-TreeEntry-Dir) |  |  |


<a name="rill-v1-TreeEntry-Dir"></a>

### TreeEntry.Dir



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `hash` | [bytes](#bytes) |  |  |
| `summary` | [DirSummary](#rill-v1-DirSummary) |  |  |


<a name="rill-v1-TreeEntry-File"></a>

### TreeEntry.File



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `content` | [ContentRef](#rill-v1-ContentRef) |  |  |
| `kind` | [FileKind](#rill-v1-FileKind) |  |  |


<a name="rill-v1-Workspace"></a>

### Workspace

A principal's in-progress state on a repository.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/workspaces/{workspace}`. Output only. |
| `uid` | [bytes](#bytes) |  | The `ws_` id. Output only. |
| `owner` | [Principal](#rill-v1-Principal) |  | Output only. |
| `display_name` | [string](#string) |  |  |
| `base` | [bytes](#bytes) |  | The version the workspace is based on. Output only. |
| `working_change` | [bytes](#bytes) |  | The change its snapshot becomes versions of. Output only. |
| `snapshot` | [bytes](#bytes) | optional | Output only. |
| `sparse` | [SparseProfile](#rill-v1-SparseProfile) |  |  |
| `head_op` | [bytes](#bytes) | optional | Output only. |
| `created_at` | [int64](#int64) |  | Output only. |
| `updated_at` | [int64](#int64) |  | Output only. |
| `etag` | [string](#string) |  | Changes with every operation; Update and Delete take it. Output only. |
| `repo_uid` | [bytes](#bytes) |  | The repository's `rp_` id. Output only. |


<a name="rill-v1-WorkspaceOperation"></a>

### WorkspaceOperation

One entry in a workspace's operation log.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/workspaces/{workspace}/operations/{operation}`. Output only. |
| `id` | [bytes](#bytes) |  |  |
| `workspace` | [bytes](#bytes) |  |  |
| `parent` | [bytes](#bytes) | optional |  |
| `actor` | [Principal](#rill-v1-Principal) |  |  |
| `kind` | [OperationKind](#rill-v1-OperationKind) |  |  |
| `at` | [int64](#int64) |  |  |
| `resulting` | [Workspace](#rill-v1-Workspace) |  | The whole workspace after this operation. |



<a name="rill-v1-CommentCategory"></a>

### CommentCategory



| Name | Number | Description |
| --- | --- | --- |
| `COMMENT_CATEGORY_UNSPECIFIED` | 0 |  |
| `COMMENT_CATEGORY_DISCUSSION` | 1 |  |
| `COMMENT_CATEGORY_BUG` | 2 |  |
| `COMMENT_CATEGORY_SECURITY` | 3 |  |
| `COMMENT_CATEGORY_STYLE` | 4 |  |
| `COMMENT_CATEGORY_TEST_GAP` | 5 |  |
| `COMMENT_CATEGORY_QUESTION` | 6 |  |
| `COMMENT_CATEGORY_SUGGESTION` | 7 |  |


<a name="rill-v1-DiffFormat"></a>

### DiffFormat

Format of hunks in a diff response.

| Name | Number | Description |
| --- | --- | --- |
| `DIFF_FORMAT_UNSPECIFIED` | 0 |  |
| `DIFF_FORMAT_STRUCTURED` | 1 |  |
| `DIFF_FORMAT_UNIFIED` | 2 |  |


<a name="rill-v1-DiffLine-Op"></a>

### DiffLine.Op



| Name | Number | Description |
| --- | --- | --- |
| `OP_UNSPECIFIED` | 0 |  |
| `OP_CONTEXT` | 1 |  |
| `OP_INSERT` | 2 |  |
| `OP_DELETE` | 3 |  |


<a name="rill-v1-FileKind"></a>

### FileKind

How a file is stored and executed.

| Name | Number | Description |
| --- | --- | --- |
| `FILE_KIND_UNSPECIFIED` | 0 |  |
| `FILE_KIND_REGULAR` | 1 |  |
| `FILE_KIND_EXECUTABLE` | 2 |  |
| `FILE_KIND_SYMLINK` | 3 |  |


<a name="rill-v1-IntentKind"></a>

### IntentKind



| Name | Number | Description |
| --- | --- | --- |
| `INTENT_KIND_UNSPECIFIED` | 0 |  |
| `INTENT_KIND_FEATURE` | 1 |  |
| `INTENT_KIND_BUG` | 2 |  |
| `INTENT_KIND_REFACTOR` | 3 |  |
| `INTENT_KIND_CHORE` | 4 |  |
| `INTENT_KIND_EXPERIMENT` | 5 |  |
| `INTENT_KIND_SYSTEM` | 6 |  |


<a name="rill-v1-IntentStatus"></a>

### IntentStatus



| Name | Number | Description |
| --- | --- | --- |
| `INTENT_STATUS_UNSPECIFIED` | 0 |  |
| `INTENT_STATUS_OPEN` | 1 |  |
| `INTENT_STATUS_IN_PROGRESS` | 2 |  |
| `INTENT_STATUS_DONE` | 3 |  |
| `INTENT_STATUS_ABANDONED` | 4 |  |


<a name="rill-v1-ObjectKind"></a>

### ObjectKind

The kind of object a hash refers to.

| Name | Number | Description |
| --- | --- | --- |
| `OBJECT_KIND_UNSPECIFIED` | 0 |  |
| `OBJECT_KIND_BLOB` | 1 |  |
| `OBJECT_KIND_CHUNK` | 2 |  |
| `OBJECT_KIND_BLOB_MANIFEST` | 3 |  |
| `OBJECT_KIND_TREE` | 4 |  |
| `OBJECT_KIND_CHANGE_VERSION` | 5 |  |
| `OBJECT_KIND_DERIVED` | 6 |  |


<a name="rill-v1-PrincipalKind"></a>

### PrincipalKind

What kind of actor a principal is.

| Name | Number | Description |
| --- | --- | --- |
| `PRINCIPAL_KIND_UNSPECIFIED` | 0 |  |
| `PRINCIPAL_KIND_HUMAN` | 1 |  |
| `PRINCIPAL_KIND_AGENT` | 2 |  |
| `PRINCIPAL_KIND_SERVICE` | 3 |  |


<a name="rill-v1-ReviewStatus"></a>

### ReviewStatus



| Name | Number | Description |
| --- | --- | --- |
| `REVIEW_STATUS_UNSPECIFIED` | 0 |  |
| `REVIEW_STATUS_OPEN` | 2 |  |
| `REVIEW_STATUS_LANDABLE` | 3 |  |
| `REVIEW_STATUS_LANDING` | 4 |  |
| `REVIEW_STATUS_LANDED` | 5 |  |
| `REVIEW_STATUS_ABANDONED` | 6 |  |


<a name="rill-v1-RiskClass"></a>

### RiskClass

Risk classification of a change.

| Name | Number | Description |
| --- | --- | --- |
| `RISK_CLASS_UNSPECIFIED` | 0 |  |
| `RISK_CLASS_TRIVIAL` | 1 |  |
| `RISK_CLASS_LOW` | 2 |  |
| `RISK_CLASS_NORMAL` | 3 |  |
| `RISK_CLASS_SENSITIVE` | 4 |  |
| `RISK_CLASS_CRITICAL` | 5 |  |



## rill/v1/access.proto

Access: the relationship tuples that say who may do what in a
repository, and questions about them.

Responses carry the repository's relation version after the call.
Clients send versions they have seen in the `x-rill-relations` header
(`rp_<hex>=<version>`, comma separated); checks on that repository then
reflect at least that version on every server.

<a name="rill-v1-AccessService"></a>

### AccessService

Relation tuples and access checks.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `CreateRelation` | [CreateRelationRequest](#rill-v1-CreateRelationRequest) | [CreateRelationResponse](#rill-v1-CreateRelationResponse) | Grant a relation (repository owners). |
| `DeleteRelation` | [DeleteRelationRequest](#rill-v1-DeleteRelationRequest) | [DeleteRelationResponse](#rill-v1-DeleteRelationResponse) | Take a relation back (repository owners). |
| `ListRelations` | [ListRelationsRequest](#rill-v1-ListRelationsRequest) | [ListRelationsResponse](#rill-v1-ListRelationsResponse) | A repository's tuples. |
| `CheckAccess` | [CheckAccessRequest](#rill-v1-CheckAccessRequest) | [CheckAccessResponse](#rill-v1-CheckAccessResponse) | Whether a subject holds a relation on a path, and which tuples say so. |
| `ExpandAccess` | [ExpandAccessRequest](#rill-v1-ExpandAccessRequest) | [ExpandAccessResponse](#rill-v1-ExpandAccessResponse) | Every tuple that grants a relation on a path, with team members. |
| `LookupPaths` | [LookupPathsRequest](#rill-v1-LookupPathsRequest) | [LookupPathsResponse](#rill-v1-LookupPathsResponse) | The directories a subject holds a relation on. |



<a name="rill-v1-CheckAccessRequest"></a>

### CheckAccessRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `path` | [string](#string) |  |  |
| `relation` | [string](#string) |  | Required. |
| `subject` | [string](#string) | optional | `user:<name>` or `agent:<name>`; the caller when absent. Asking about anyone else needs owner. |


<a name="rill-v1-CheckAccessResponse"></a>

### CheckAccessResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `allowed` | [bool](#bool) |  |  |
| `because` | [Relation](#rill-v1-Relation) | repeated | The tuples that grant it, nearest directory first. |
| `org_admin` | [bool](#bool) |  | Allowed as an administrator of the repository's organization. |
| `relation_version` | [int64](#int64) |  |  |


<a name="rill-v1-CreateRelationRequest"></a>

### CreateRelationRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `relation` | [Relation](#rill-v1-Relation) |  | Required. |


<a name="rill-v1-CreateRelationResponse"></a>

### CreateRelationResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo_uid` | [string](#string) |  | The repository's uid (`rp_…`), for `x-rill-relations`. |
| `relation_version` | [int64](#int64) |  |  |


<a name="rill-v1-DeleteRelationRequest"></a>

### DeleteRelationRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `relation` | [Relation](#rill-v1-Relation) |  | Required. |


<a name="rill-v1-DeleteRelationResponse"></a>

### DeleteRelationResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo_uid` | [string](#string) |  |  |
| `relation_version` | [int64](#int64) |  |  |


<a name="rill-v1-ExpandAccessRequest"></a>

### ExpandAccessRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `path` | [string](#string) |  |  |
| `relation` | [string](#string) |  | Required. |


<a name="rill-v1-ExpandAccessResponse"></a>

### ExpandAccessResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `tuples` | [ExpandedTuple](#rill-v1-ExpandedTuple) | repeated |  |
| `relation_version` | [int64](#int64) |  |  |


<a name="rill-v1-ExpandedTuple"></a>

### ExpandedTuple



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `tuple` | [Relation](#rill-v1-Relation) |  |  |
| `members` | [Principal](#rill-v1-Principal) | repeated | For a team subject, the org's active users in the team. |


<a name="rill-v1-ListRelationsRequest"></a>

### ListRelationsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `filter` | [string](#string) |  | On `path`, `subject` and `relation`. |


<a name="rill-v1-ListRelationsResponse"></a>

### ListRelationsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `relations` | [Relation](#rill-v1-Relation) | repeated |  |
| `relation_version` | [int64](#int64) |  |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-LookupPathsRequest"></a>

### LookupPathsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `relation` | [string](#string) |  | Required. |
| `subject` | [string](#string) | optional | As in CheckAccessRequest. |


<a name="rill-v1-LookupPathsResponse"></a>

### LookupPathsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `paths` | [string](#string) | repeated | Directories (each with its subtree); `""` is the whole repository. |
| `relation_version` | [int64](#int64) |  |  |


<a name="rill-v1-Relation"></a>

### Relation

A ReBAC tuple: `subject` holds `relation` on `path` (the whole
repository when empty).

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `path` | [string](#string) |  |  |
| `relation` | [string](#string) |  | `owner`, `writer`, `reviewer` or `reader`; each implies the next. |
| `subject` | [string](#string) |  | `user:<name>`, `agent:<name>`, `team:<name>`, `kind:<human|agent|service>`, `principal:pr_<hex>` or `*`. Users and agents are stored by principal id. |




## rill/v1/admin.proto

Admin: the server's health, and importing existing history.

<a name="rill-v1-AdminService"></a>

### AdminService

Instance operations.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `GetHealth` | [GetHealthRequest](#rill-v1-GetHealthRequest) | [GetHealthResponse](#rill-v1-GetHealthResponse) | The server's components and whether each is healthy. |
| `ImportTrunkPosition` | [ImportTrunkPositionRequest](#rill-v1-ImportTrunkPositionRequest) | [ImportTrunkPositionResponse](#rill-v1-ImportTrunkPositionResponse) | Append an imported version to trunk, for importers of existing history (repository owners). |



<a name="rill-v1-ComponentHealth"></a>

### ComponentHealth



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  |  |
| `ok` | [bool](#bool) |  |  |
| `detail` | [string](#string) | optional |  |


<a name="rill-v1-GetHealthRequest"></a>

### GetHealthRequest




<a name="rill-v1-GetHealthResponse"></a>

### GetHealthResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `ok` | [bool](#bool) |  |  |
| `version` | [string](#string) |  |  |
| `components` | [ComponentHealth](#rill-v1-ComponentHealth) | repeated |  |
| `attestation_key` | [string](#string) |  | Hex compressed P-256 public key commit attestations are signed with. |


<a name="rill-v1-ImportTrunkPositionRequest"></a>

### ImportTrunkPositionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `expected` | [bytes](#bytes) | optional | The trunk head the version goes on; absent for an empty trunk. |
| `version` | [bytes](#bytes) |  | Required. |


<a name="rill-v1-ImportTrunkPositionResponse"></a>

### ImportTrunkPositionResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `position` | [uint64](#uint64) |  |  |




## rill/v1/attention.proto

Human attention: what needs a given person, sampled audits of changes
that landed on agent review alone, batch review of look-alike changes,
and daily digests per intent.

<a name="rill-v1-AttentionService"></a>

### AttentionService

Routing human attention.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `ListInbox` | [ListInboxRequest](#rill-v1-ListInboxRequest) | [ListInboxResponse](#rill-v1-ListInboxResponse) | What waits on the caller in a repository, most urgent first. |
| `ListAudits` | [ListAuditsRequest](#rill-v1-ListAuditsRequest) | [ListAuditsResponse](#rill-v1-ListAuditsResponse) | Audits still waiting for a human. |
| `RecordAudit` | [RecordAuditRequest](#rill-v1-RecordAuditRequest) | [RecordAuditResponse](#rill-v1-RecordAuditResponse) | Pass or fail an audit (an assignee or a repository owner). A failure counts against the agents that approved the change and can revert it. |
| `ListSimilarChanges` | [ListSimilarChangesRequest](#rill-v1-ListSimilarChangesRequest) | [ListSimilarChangesResponse](#rill-v1-ListSimilarChangesResponse) | An intent's changes in review, grouped by the edit they make. |
| `BatchVote` | [BatchVoteRequest](#rill-v1-BatchVoteRequest) | [BatchVoteResponse](#rill-v1-BatchVoteResponse) | Vote on every change of a group at once; each must still make the group's edit at the version it was grouped at. |
| `GetIntentDigest` | [GetIntentDigestRequest](#rill-v1-GetIntentDigestRequest) | [GetIntentDigestResponse](#rill-v1-GetIntentDigestResponse) | What happened under an intent on one day. |



<a name="rill-v1-Audit"></a>

### Audit



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landing` | [string](#string) |  | `…/trunk/positions/{n}`. |
| `change` | [string](#string) |  |  |
| `version` | [string](#string) |  |  |
| `assignees` | [string](#string) | repeated | `user:…`, `team:…`, or `owners`. |
| `risk` | [string](#string) |  |  |
| `status` | [string](#string) |  | `pending`, `passed` or `failed`. |
| `sampled_at` | [int64](#int64) |  |  |
| `decided_by` | [Principal](#rill-v1-Principal) |  |  |
| `decided_at` | [int64](#int64) |  |  |
| `note` | [string](#string) |  |  |


<a name="rill-v1-BatchVoteRequest"></a>

### BatchVoteRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `intent` | [string](#string) |  | `…/intents/{intent}`. Required. |
| `signature` | [string](#string) |  | The group voted on. Required. |
| `label` | [string](#string) |  |  |
| `value` | [int32](#int32) |  |  |
| `reason` | [string](#string) |  |  |


<a name="rill-v1-BatchVoteResponse"></a>

### BatchVoteResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `results` | [BatchVoteResult](#rill-v1-BatchVoteResult) | repeated |  |


<a name="rill-v1-BatchVoteResult"></a>

### BatchVoteResult



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [string](#string) |  |  |
| `voted` | [bool](#bool) |  |  |
| `error` | [string](#string) |  | Why not, when not. |


<a name="rill-v1-GetIntentDigestRequest"></a>

### GetIntentDigestRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent` | [string](#string) |  | `…/intents/{intent}`. Required. |
| `day` | [string](#string) |  | `YYYY-MM-DD`, UTC; default today. |


<a name="rill-v1-GetIntentDigestResponse"></a>

### GetIntentDigestResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `day` | [string](#string) |  |  |
| `landed` | [uint32](#uint32) |  |  |
| `reverted` | [uint32](#uint32) |  |  |
| `flagged` | [uint32](#uint32) |  |  |
| `sampled` | [uint32](#uint32) |  |  |
| `open` | [uint32](#uint32) |  |  |


<a name="rill-v1-InboxItem"></a>

### InboxItem



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `kind` | [InboxKind](#rill-v1-InboxKind) |  |  |
| `change` | [string](#string) |  | `…/changes/{change}`, when about a change. |
| `intent` | [string](#string) |  | `…/intents/{intent}`, when about an intent. |
| `landing` | [string](#string) |  | `…/trunk/positions/{n}`, when about a landing. |
| `title` | [string](#string) |  |  |
| `detail` | [string](#string) |  |  |
| `since` | [int64](#int64) |  |  |
| `urgency` | [uint32](#uint32) |  |  |


<a name="rill-v1-ListAuditsRequest"></a>

### ListAuditsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |


<a name="rill-v1-ListAuditsResponse"></a>

### ListAuditsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `audits` | [Audit](#rill-v1-Audit) | repeated |  |


<a name="rill-v1-ListInboxRequest"></a>

### ListInboxRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `limit` | [uint32](#uint32) |  | At most this many items (default 50, at most 500). |


<a name="rill-v1-ListInboxResponse"></a>

### ListInboxResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `items` | [InboxItem](#rill-v1-InboxItem) | repeated |  |


<a name="rill-v1-ListSimilarChangesRequest"></a>

### ListSimilarChangesRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent` | [string](#string) |  | `…/intents/{intent}`. Required. |


<a name="rill-v1-ListSimilarChangesResponse"></a>

### ListSimilarChangesResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `groups` | [SimilarGroup](#rill-v1-SimilarGroup) | repeated |  |


<a name="rill-v1-RecordAuditRequest"></a>

### RecordAuditRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `landing` | [string](#string) |  | `…/trunk/positions/{n}`. Required. |
| `passed` | [bool](#bool) |  |  |
| `note` | [string](#string) |  |  |
| `revert` | [bool](#bool) |  | On failure, also revert the landing. |


<a name="rill-v1-RecordAuditResponse"></a>

### RecordAuditResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `audit` | [Audit](#rill-v1-Audit) |  |  |


<a name="rill-v1-SimilarGroup"></a>

### SimilarGroup



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `signature` | [string](#string) |  |  |
| `changes` | [string](#string) | repeated | `…/changes/{change}`, and the versions they were grouped at. |
| `versions` | [string](#string) | repeated |  |



<a name="rill-v1-InboxKind"></a>

### InboxKind



| Name | Number | Description |
| --- | --- | --- |
| `INBOX_KIND_UNSPECIFIED` | 0 |  |
| `INBOX_KIND_ESCALATION` | 1 |  |
| `INBOX_KIND_AUDIT` | 2 |  |
| `INBOX_KIND_OWNER_REVIEW` | 3 |  |
| `INBOX_KIND_INTENT_APPROVAL` | 4 |  |
| `INBOX_KIND_THREAD` | 5 |  |



## rill/v1/audit.proto

Audit: each org's hash-chained audit log and its signed seals, for org
administrators (instance administrators for `orgs/_instance`).

<a name="rill-v1-AuditService"></a>

### AuditService

The audit log.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `ListAuditEntries` | [ListAuditEntriesRequest](#rill-v1-ListAuditEntriesRequest) | [ListAuditEntriesResponse](#rill-v1-ListAuditEntriesResponse) | Entries in chain order. |
| `ListAuditRoots` | [ListAuditRootsRequest](#rill-v1-ListAuditRootsRequest) | [ListAuditRootsResponse](#rill-v1-ListAuditRootsResponse) | The signed seals over the chain, oldest first. |
| `SealAudit` | [SealAuditRequest](#rill-v1-SealAuditRequest) | [SealAuditResponse](#rill-v1-SealAuditResponse) | Seal what the chain gained since its last seal, now. |
| `ListAuditKeys` | [ListAuditKeysRequest](#rill-v1-ListAuditKeysRequest) | [ListAuditKeysResponse](#rill-v1-ListAuditKeysResponse) | The public keys seals verify with, by token key id. |
| `WatchAudit` | [WatchAuditRequest](#rill-v1-WatchAuditRequest) | [WatchAuditResponse](#rill-v1-WatchAuditResponse) stream | Entries as they are chained, for SIEM export. |



<a name="rill-v1-AuditEntryRecord"></a>

### AuditEntryRecord



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `seq` | [uint64](#uint64) |  |  |
| `org` | [string](#string) |  |  |
| `at` | [int64](#int64) |  |  |
| `actor` | [Principal](#rill-v1-Principal) |  |  |
| `action` | [string](#string) |  | Dotted, e.g. `relation.add`, `token.issue`, `land`. |
| `target` | [string](#string) |  |  |
| `allowed` | [bool](#bool) |  |  |
| `denied_reason` | [string](#string) | optional |  |
| `detail` | [AuditEntryRecord.DetailEntry](#rill-v1-AuditEntryRecord-DetailEntry) | repeated |  |
| `hash` | [bytes](#bytes) |  |  |
| `prev` | [bytes](#bytes) |  |  |
| `encoded` | [bytes](#bytes) |  | The entry as hashed (postcard), so a verifier recomputes the chain from exactly what was stored. |


<a name="rill-v1-AuditEntryRecord-DetailEntry"></a>

### AuditEntryRecord.DetailEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key` | [string](#string) |  |  |
| `value` | [string](#string) |  |  |


<a name="rill-v1-AuditKey"></a>

### AuditKey



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `id` | [uint32](#uint32) |  |  |
| `public_key` | [bytes](#bytes) |  |  |


<a name="rill-v1-AuditRootRecord"></a>

### AuditRootRecord



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `first_seq` | [uint64](#uint64) |  |  |
| `last_seq` | [uint64](#uint64) |  |  |
| `head` | [bytes](#bytes) |  |  |
| `export` | [bytes](#bytes) |  | The blob, in the org's object store, holding the sealed entries as JSON lines. |
| `sealed_at` | [int64](#int64) |  |  |
| `key_id` | [uint32](#uint32) |  |  |
| `encoded` | [bytes](#bytes) |  | The root as signed (postcard). |


<a name="rill-v1-ListAuditEntriesRequest"></a>

### ListAuditEntriesRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`; `orgs/_instance` for instance-wide actions. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `after_seq` | [uint64](#uint64) |  | Entries after this sequence on the first page. |


<a name="rill-v1-ListAuditEntriesResponse"></a>

### ListAuditEntriesResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `entries` | [AuditEntryRecord](#rill-v1-AuditEntryRecord) | repeated |  |
| `head_seq` | [uint64](#uint64) |  |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListAuditKeysRequest"></a>

### ListAuditKeysRequest




<a name="rill-v1-ListAuditKeysResponse"></a>

### ListAuditKeysResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `keys` | [AuditKey](#rill-v1-AuditKey) | repeated |  |


<a name="rill-v1-ListAuditRootsRequest"></a>

### ListAuditRootsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListAuditRootsResponse"></a>

### ListAuditRootsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `roots` | [AuditRootRecord](#rill-v1-AuditRootRecord) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-SealAuditRequest"></a>

### SealAuditRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |


<a name="rill-v1-SealAuditResponse"></a>

### SealAuditResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `root` | [AuditRootRecord](#rill-v1-AuditRootRecord) | optional | Absent when nothing was added since the last seal. |


<a name="rill-v1-WatchAuditRequest"></a>

### WatchAuditRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `after_seq` | [uint64](#uint64) |  | Entries after this one; 0 for the whole chain. |


<a name="rill-v1-WatchAuditResponse"></a>

### WatchAuditResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `entry` | [AuditEntryRecord](#rill-v1-AuditEntryRecord) |  |  |




## rill/v1/changes.proto

Changes and their versions: reading them and writing new ones.

A change is `orgs/{org}/repos/{repo}/changes/{change}` (`ch_` id); a
version is `orgs/{org}/repos/{repo}/versions/{hash}`, its content hash.

<a name="rill-v1-ChangeService"></a>

### ChangeService

Change versions: read, create, edit, rebase, merge and resolve.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `GetVersion` | [GetVersionRequest](#rill-v1-GetVersionRequest) | [GetVersionResponse](#rill-v1-GetVersionResponse) | One version: its metadata and, with `view = FULL`, its changes. |
| `BatchGetVersions` | [BatchGetVersionsRequest](#rill-v1-BatchGetVersionsRequest) | [BatchGetVersionsResponse](#rill-v1-BatchGetVersionsResponse) | Up to 1,000 versions. |
| `ListVersions` | [ListVersionsRequest](#rill-v1-ListVersionsRequest) | [ListVersionsResponse](#rill-v1-ListVersionsResponse) | A change's versions, oldest first; the last is current. |
| `CreateVersion` | [CreateVersionRequest](#rill-v1-CreateVersionRequest) | [CreateVersionResponse](#rill-v1-CreateVersionResponse) | Store a version built by the client. |
| `ApplyEdits` | [ApplyEditsRequest](#rill-v1-ApplyEditsRequest) | [ApplyEditsResponse](#rill-v1-ApplyEditsResponse) | Build and store a version from edits to a base. |
| `RebaseVersion` | [RebaseVersionRequest](#rill-v1-RebaseVersionRequest) | [RebaseVersionResponse](#rill-v1-RebaseVersionResponse) | A new version of a change on a new parent. |
| `MergeVersions` | [MergeVersionsRequest](#rill-v1-MergeVersionsRequest) | [MergeVersionsResponse](#rill-v1-MergeVersionsResponse) | A merge of two revisions. |
| `ResolveConflicts` | [ResolveConflictsRequest](#rill-v1-ResolveConflictsRequest) | [ResolveConflictsResponse](#rill-v1-ResolveConflictsResponse) | A new version with some of a version's conflicts resolved. |



<a name="rill-v1-ApplyEditsRequest"></a>

### ApplyEditsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `base` | [EditBase](#rill-v1-EditBase) |  |  |
| `edits` | [Edit](#rill-v1-Edit) | repeated |  |
| `message` | [string](#string) |  |  |
| `intent` | [string](#string) |  | `orgs/{org}/repos/{repo}/intents/{intent}`. |
| `provenance` | [Provenance](#rill-v1-Provenance) |  |  |
| `change` | [string](#string) |  | Amend this change (`…/changes/{change}`) rather than starting one. |


<a name="rill-v1-ApplyEditsResponse"></a>

### ApplyEditsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `info` | [ChangeInfo](#rill-v1-ChangeInfo) |  |  |
| `version` | [ChangeVersion](#rill-v1-ChangeVersion) |  |  |


<a name="rill-v1-BatchGetVersionsRequest"></a>

### BatchGetVersionsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `names` | [string](#string) | repeated | At most 1,000 version names. |
| `view` | [VersionView](#rill-v1-VersionView) |  |  |


<a name="rill-v1-BatchGetVersionsResponse"></a>

### BatchGetVersionsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `versions` | [Version](#rill-v1-Version) | repeated | Parallel to `names`. |


<a name="rill-v1-CreateVersionRequest"></a>

### CreateVersionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `version` | [ChangeVersion](#rill-v1-ChangeVersion) |  | Required. |
| `signatures` | [Signature](#rill-v1-Signature) | repeated |  |


<a name="rill-v1-CreateVersionResponse"></a>

### CreateVersionResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `version` | [ChangeInfo](#rill-v1-ChangeInfo) |  |  |


<a name="rill-v1-Edit"></a>

### Edit

One edit to apply to a path.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `path` | [string](#string) |  |  |
| `set_content` | [SetContent](#rill-v1-SetContent) |  |  |
| `delete` | [bool](#bool) |  |  |
| `patch` | [string](#string) |  | A unified diff against the base content. |


<a name="rill-v1-EditBase"></a>

### EditBase

What an edit starts from: a revision, or a change's current version.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `revision` | [string](#string) |  |  |
| `change` | [string](#string) |  | `orgs/{org}/repos/{repo}/changes/{change}`. |


<a name="rill-v1-GetVersionRequest"></a>

### GetVersionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/versions/{hash}`. Required. |
| `view` | [VersionView](#rill-v1-VersionView) |  |  |


<a name="rill-v1-GetVersionResponse"></a>

### GetVersionResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `version` | [Version](#rill-v1-Version) |  |  |


<a name="rill-v1-ListVersionsRequest"></a>

### ListVersionsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}/changes/{change}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListVersionsResponse"></a>

### ListVersionsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `versions` | [ChangeInfo](#rill-v1-ChangeInfo) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-MergeVersionsRequest"></a>

### MergeVersionsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `left` | [string](#string) |  | Required. |
| `right` | [string](#string) |  | Required. |
| `message` | [string](#string) |  |  |
| `intent` | [string](#string) |  |  |
| `provenance` | [Provenance](#rill-v1-Provenance) |  |  |


<a name="rill-v1-MergeVersionsResponse"></a>

### MergeVersionsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `info` | [ChangeInfo](#rill-v1-ChangeInfo) |  |  |
| `version` | [ChangeVersion](#rill-v1-ChangeVersion) |  |  |


<a name="rill-v1-RebaseVersionRequest"></a>

### RebaseVersionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | The version to rebase: `…/versions/{hash}`. Required. |
| `onto` | [string](#string) |  | The revision to rebase onto. Required. |


<a name="rill-v1-RebaseVersionResponse"></a>

### RebaseVersionResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `info` | [ChangeInfo](#rill-v1-ChangeInfo) |  |  |
| `version` | [ChangeVersion](#rill-v1-ChangeVersion) |  |  |


<a name="rill-v1-Resolution"></a>

### Resolution



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `path` | [string](#string) |  |  |
| `set` | [SetContent](#rill-v1-SetContent) |  |  |
| `delete` | [bool](#bool) |  |  |


<a name="rill-v1-ResolveConflictsRequest"></a>

### ResolveConflictsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | The conflicted version: `…/versions/{hash}`. Required. |
| `resolutions` | [Resolution](#rill-v1-Resolution) | repeated |  |
| `provenance` | [Provenance](#rill-v1-Provenance) |  |  |


<a name="rill-v1-ResolveConflictsResponse"></a>

### ResolveConflictsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `info` | [ChangeInfo](#rill-v1-ChangeInfo) |  |  |
| `version` | [ChangeVersion](#rill-v1-ChangeVersion) |  |  |


<a name="rill-v1-SetContent"></a>

### SetContent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `content` | [ContentRef](#rill-v1-ContentRef) |  |  |
| `kind` | [FileKind](#rill-v1-FileKind) |  |  |
| `copy_from` | [CopySource](#rill-v1-CopySource) | optional |  |


<a name="rill-v1-Version"></a>

### Version

A version as returned.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `info` | [ChangeInfo](#rill-v1-ChangeInfo) |  |  |
| `version` | [ChangeVersion](#rill-v1-ChangeVersion) |  | Set for VERSION_VIEW_FULL. |
| `signatures` | [Signature](#rill-v1-Signature) | repeated |  |
| `read_grant` | [string](#string) |  | A read grant for the content the version names. |



<a name="rill-v1-VersionView"></a>

### VersionView

How much of a version to return.

| Name | Number | Description |
| --- | --- | --- |
| `VERSION_VIEW_UNSPECIFIED` | 0 | Metadata only. |
| `VERSION_VIEW_BASIC` | 1 |  |
| `VERSION_VIEW_FULL` | 2 | Metadata, the file and subtree changes (those the caller may read), signatures and a read grant. |



## rill/v1/checks.proto

Checks: what verification ran, test by test, and which targets are
quarantined.

A run is `orgs/{org}/repos/{repo}/runs/{run}`; a quarantine is
`orgs/{org}/repos/{repo}/quarantines/{target}`.

<a name="rill-v1-ChecksService"></a>

### ChecksService

Verification runs, test history, flakes and quarantines.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `GetTestHistory` | [GetTestHistoryRequest](#rill-v1-GetTestHistoryRequest) | [GetTestHistoryResponse](#rill-v1-GetTestHistoryResponse) | Recent runs of a step or test, newest first, with its flake stats. |
| `ListFlakyTests` | [ListFlakyTestsRequest](#rill-v1-ListFlakyTestsRequest) | [ListFlakyTestsResponse](#rill-v1-ListFlakyTestsResponse) | Targets whose outcome changes on unchanged inputs, flakiest first. |
| `ListRuns` | [ListRunsRequest](#rill-v1-ListRunsRequest) | [ListRunsResponse](#rill-v1-ListRunsResponse) | Verification runs, newest first. |
| `GetRun` | [GetRunRequest](#rill-v1-GetRunRequest) | [GetRunResponse](#rill-v1-GetRunResponse) | One run with its steps. |
| `WatchRun` | [WatchRunRequest](#rill-v1-WatchRunRequest) | [WatchRunResponse](#rill-v1-WatchRunResponse) stream | The run now and after every change, until it ends. |
| `ReadLog` | [ReadLogRequest](#rill-v1-ReadLogRequest) | [ReadLogResponse](#rill-v1-ReadLogResponse) | A byte range of a step's standard output or error. |
| `ReadArtifact` | [ReadArtifactRequest](#rill-v1-ReadArtifactRequest) | [ReadArtifactResponse](#rill-v1-ReadArtifactResponse) | A byte range of a file a step produced. |
| `ListNonhermeticSteps` | [ListNonhermeticStepsRequest](#rill-v1-ListNonhermeticStepsRequest) | [ListNonhermeticStepsResponse](#rill-v1-ListNonhermeticStepsResponse) | Steps a hermeticity probe caught giving different outputs on a re-run; they run uncached until the flag expires. |
| `ListQuarantines` | [ListQuarantinesRequest](#rill-v1-ListQuarantinesRequest) | [ListQuarantinesResponse](#rill-v1-ListQuarantinesResponse) | Targets whose failures neither block landings nor make trunk red. |
| `CreateQuarantine` | [CreateQuarantineRequest](#rill-v1-CreateQuarantineRequest) | [CreateQuarantineResponse](#rill-v1-CreateQuarantineResponse) | Quarantine a step or test (repository owners). |
| `DeleteQuarantine` | [DeleteQuarantineRequest](#rill-v1-DeleteQuarantineRequest) | [DeleteQuarantineResponse](#rill-v1-DeleteQuarantineResponse) | Lift a quarantine (repository owners). |



<a name="rill-v1-Artifact"></a>

### Artifact



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `path` | [string](#string) |  |  |
| `size` | [uint64](#uint64) |  |  |


<a name="rill-v1-CreateQuarantineRequest"></a>

### CreateQuarantineRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `quarantine` | [Quarantine](#rill-v1-Quarantine) |  | Required. |


<a name="rill-v1-CreateQuarantineResponse"></a>

### CreateQuarantineResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `quarantine` | [Quarantine](#rill-v1-Quarantine) |  |  |


<a name="rill-v1-DeleteQuarantineRequest"></a>

### DeleteQuarantineRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/quarantines/{target}`. Required. |


<a name="rill-v1-DeleteQuarantineResponse"></a>

### DeleteQuarantineResponse




<a name="rill-v1-FlakyTest"></a>

### FlakyTest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `target` | [string](#string) |  |  |
| `score` | [double](#double) |  | 0 to 1: the decayed rate of changed outcomes on unchanged inputs. |
| `runs` | [double](#double) |  |  |
| `failures` | [double](#double) |  |  |
| `flips` | [double](#double) |  |  |
| `last` | [TestOutcome](#rill-v1-TestOutcome) |  |  |


<a name="rill-v1-GetRunRequest"></a>

### GetRunRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/runs/{run}`. Required. |


<a name="rill-v1-GetRunResponse"></a>

### GetRunResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `run` | [VerifyRun](#rill-v1-VerifyRun) |  |  |


<a name="rill-v1-GetTestHistoryRequest"></a>

### GetTestHistoryRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `target` | [string](#string) |  | Required. |
| `max_results` | [int32](#int32) |  | Default 50, at most 100. |


<a name="rill-v1-GetTestHistoryResponse"></a>

### GetTestHistoryResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `runs` | [TestRun](#rill-v1-TestRun) | repeated |  |
| `stats` | [FlakyTest](#rill-v1-FlakyTest) |  |  |


<a name="rill-v1-ListFlakyTestsRequest"></a>

### ListFlakyTestsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `min_score` | [double](#double) | optional | Default 0.1. |


<a name="rill-v1-ListFlakyTestsResponse"></a>

### ListFlakyTestsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `tests` | [FlakyTest](#rill-v1-FlakyTest) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListNonhermeticStepsRequest"></a>

### ListNonhermeticStepsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListNonhermeticStepsResponse"></a>

### ListNonhermeticStepsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `steps` | [NonhermeticStep](#rill-v1-NonhermeticStep) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListQuarantinesRequest"></a>

### ListQuarantinesRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListQuarantinesResponse"></a>

### ListQuarantinesResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `quarantines` | [Quarantine](#rill-v1-Quarantine) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListRunsRequest"></a>

### ListRunsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `version` | [string](#string) |  | Only runs of this version, `…/versions/{hash}`. |


<a name="rill-v1-ListRunsResponse"></a>

### ListRunsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `runs` | [VerifyRun](#rill-v1-VerifyRun) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-NonhermeticStep"></a>

### NonhermeticStep



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `step` | [string](#string) |  |  |
| `action` | [bytes](#bytes) |  | The action whose two runs differed. |
| `differences` | [string](#string) | repeated |  |
| `found_at` | [int64](#int64) |  |  |
| `until` | [int64](#int64) |  |  |


<a name="rill-v1-Quarantine"></a>

### Quarantine

A target whose failures are not held against changes.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/quarantines/{target}`. Output only. |
| `target` | [string](#string) |  | `step:<name>[#shard]` or `test:<id>`. Required on create. |
| `since` | [int64](#int64) |  | Output only. |
| `by` | [Principal](#rill-v1-Principal) |  | Output only. |
| `reason` | [string](#string) |  |  |
| `intent` | [bytes](#bytes) | optional | The intent tracking the fix, when one was opened. Output only. |


<a name="rill-v1-ReadArtifactRequest"></a>

### ReadArtifactRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `run` | [string](#string) |  | `…/runs/{run}`. Required. |
| `step` | [uint32](#uint32) |  |  |
| `path` | [string](#string) |  |  |
| `offset` | [int64](#int64) |  |  |
| `limit` | [uint32](#uint32) |  |  |


<a name="rill-v1-ReadArtifactResponse"></a>

### ReadArtifactResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `data` | [bytes](#bytes) |  |  |
| `offset` | [uint64](#uint64) |  |  |
| `size` | [uint64](#uint64) |  |  |


<a name="rill-v1-ReadLogRequest"></a>

### ReadLogRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `run` | [string](#string) |  | `…/runs/{run}`. Required. |
| `step` | [uint32](#uint32) |  |  |
| `stream` | [LogStream](#rill-v1-LogStream) |  |  |
| `offset` | [int64](#int64) |  | A negative offset counts from the end: -4096 is the last 4 KiB. |
| `limit` | [uint32](#uint32) |  | Default and most 1 MiB. |


<a name="rill-v1-ReadLogResponse"></a>

### ReadLogResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `data` | [bytes](#bytes) |  |  |
| `offset` | [uint64](#uint64) |  | Where `data` starts, and the whole size. |
| `size` | [uint64](#uint64) |  |  |


<a name="rill-v1-RunStep"></a>

### RunStep



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `index` | [uint32](#uint32) |  | Its index in the run, for ReadLog and ReadArtifact. |
| `name` | [string](#string) |  |  |
| `shard` | [uint32](#uint32) |  | 1-based shard and shard count, 0 when not sharded. |
| `shards` | [uint32](#uint32) |  |  |
| `state` | [StepState](#rill-v1-StepState) |  |  |
| `exit_code` | [int32](#int32) | optional |  |
| `error` | [string](#string) |  | Why it could not run. |
| `cached` | [bool](#bool) |  |  |
| `stdout_size` | [uint64](#uint64) |  |  |
| `stderr_size` | [uint64](#uint64) |  |  |
| `artifacts` | [Artifact](#rill-v1-Artifact) | repeated |  |
| `worker` | [string](#string) |  |  |
| `duration_ms` | [uint64](#uint64) | optional |  |
| `nonhermetic` | [bool](#bool) |  | It ran uncached: a probe found it non-hermetic. |


<a name="rill-v1-TestRun"></a>

### TestRun



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `target` | [string](#string) |  | `step:<name>[#i/n]` or `test:<class>::<name>`. |
| `outcome` | [TestOutcome](#rill-v1-TestOutcome) |  |  |
| `retried` | [bool](#bool) |  | The test runner retried it before it passed. |
| `duration_ms` | [uint64](#uint64) | optional |  |
| `action` | [bytes](#bytes) |  | The action it ran in; the same action means the same inputs. |
| `worker` | [string](#string) |  |  |
| `version` | [bytes](#bytes) |  |  |
| `trunk` | [bool](#bool) |  | Verified after landing. |
| `at` | [int64](#int64) |  |  |


<a name="rill-v1-VerifyRun"></a>

### VerifyRun



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/runs/{run}`. |
| `id` | [string](#string) |  |  |
| `version` | [bytes](#bytes) |  |  |
| `trunk` | [bool](#bool) |  | Verification of trunk, after landing. |
| `started_at` | [int64](#int64) |  |  |
| `green` | [bool](#bool) | optional | Unset while it runs. |
| `summary` | [string](#string) |  |  |
| `finished_at` | [int64](#int64) | optional |  |
| `steps` | [RunStep](#rill-v1-RunStep) | repeated | Empty in listings. |


<a name="rill-v1-WatchRunRequest"></a>

### WatchRunRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/runs/{run}`. Required. |


<a name="rill-v1-WatchRunResponse"></a>

### WatchRunResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `run` | [VerifyRun](#rill-v1-VerifyRun) |  |  |



<a name="rill-v1-LogStream"></a>

### LogStream



| Name | Number | Description |
| --- | --- | --- |
| `LOG_STREAM_UNSPECIFIED` | 0 |  |
| `LOG_STREAM_STDOUT` | 1 |  |
| `LOG_STREAM_STDERR` | 2 |  |


<a name="rill-v1-StepState"></a>

### StepState



| Name | Number | Description |
| --- | --- | --- |
| `STEP_STATE_UNSPECIFIED` | 0 |  |
| `STEP_STATE_QUEUED` | 1 |  |
| `STEP_STATE_RUNNING` | 2 |  |
| `STEP_STATE_PASSED` | 3 |  |
| `STEP_STATE_FAILED` | 4 |  |
| `STEP_STATE_TIMED_OUT` | 5 |  |
| `STEP_STATE_ERROR` | 6 | It could not run. |
| `STEP_STATE_UNAFFECTED` | 7 | Not run: the change touched none of its inputs. |
| `STEP_STATE_NOT_GATING` | 8 | Not run: it does not gate the change's risk class. |


<a name="rill-v1-TestOutcome"></a>

### TestOutcome



| Name | Number | Description |
| --- | --- | --- |
| `TEST_OUTCOME_UNSPECIFIED` | 0 |  |
| `TEST_OUTCOME_PASSED` | 1 |  |
| `TEST_OUTCOME_FAILED` | 2 |  |
| `TEST_OUTCOME_SKIPPED` | 3 |  |
| `TEST_OUTCOME_ERROR` | 4 | It could not run; says nothing about the test. |



## rill/v1/content.proto

Reading a repository's content at a revision: files, directories,
blobs, diffs, blame, history and search.

Requests name the repository (`orgs/{org}/repos/{repo}`) and a revision
(see repos.proto; `trunk` when empty). Content is served by hash only
with a read grant from the response that revealed the hash.

<a name="rill-v1-ContentService"></a>

### ContentService

Files, trees, blobs, diffs, blame, history and search.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `ReadFile` | [ReadFileRequest](#rill-v1-ReadFileRequest) | [ReadFileResponse](#rill-v1-ReadFileResponse) | One file's entry and, up to `max_bytes`, its content. |
| `BatchReadFiles` | [BatchReadFilesRequest](#rill-v1-BatchReadFilesRequest) | [BatchReadFilesResponse](#rill-v1-BatchReadFilesResponse) | Up to 1,000 files at one revision. |
| `Stat` | [StatRequest](#rill-v1-StatRequest) | [StatResponse](#rill-v1-StatResponse) | What is at a path: a file, a directory, a symlink, or nothing. |
| `ListDirectory` | [ListDirectoryRequest](#rill-v1-ListDirectoryRequest) | [ListDirectoryResponse](#rill-v1-ListDirectoryResponse) | A directory's entries, by name. |
| `GetTree` | [GetTreeRequest](#rill-v1-GetTreeRequest) | [GetTreeResponse](#rill-v1-GetTreeResponse) | A tree object by hash. |
| `ReadBlob` | [ReadBlobRequest](#rill-v1-ReadBlobRequest) | [ReadBlobResponse](#rill-v1-ReadBlobResponse) stream | Content by reference, streamed. |
| `WriteBlob` | [WriteBlobRequest](#rill-v1-WriteBlobRequest) stream | [WriteBlobResponse](#rill-v1-WriteBlobResponse) | Upload content: one header message, then the data. |
| `BatchCheckObjects` | [BatchCheckObjectsRequest](#rill-v1-BatchCheckObjectsRequest) | [BatchCheckObjectsResponse](#rill-v1-BatchCheckObjectsResponse) | Which objects exist, before an upload. |
| `Diff` | [DiffRequest](#rill-v1-DiffRequest) | [DiffResponse](#rill-v1-DiffResponse) | The difference between two revisions. |
| `Blame` | [BlameRequest](#rill-v1-BlameRequest) | [BlameResponse](#rill-v1-BlameResponse) | Who last changed each line of a file. |
| `ListHistory` | [ListHistoryRequest](#rill-v1-ListHistoryRequest) | [ListHistoryResponse](#rill-v1-ListHistoryResponse) | The versions behind a revision, newest first. |
| `ListPathHistory` | [ListPathHistoryRequest](#rill-v1-ListPathHistoryRequest) | [ListPathHistoryResponse](#rill-v1-ListPathHistoryResponse) | The versions that changed one path, following renames. |
| `Grep` | [GrepRequest](#rill-v1-GrepRequest) | [GrepResponse](#rill-v1-GrepResponse) | Regular-expression search over a revision's files. |
| `FindMergeBase` | [FindMergeBaseRequest](#rill-v1-FindMergeBaseRequest) | [FindMergeBaseResponse](#rill-v1-FindMergeBaseResponse) | The merge bases of two revisions. |
| `IsAncestor` | [IsAncestorRequest](#rill-v1-IsAncestorRequest) | [IsAncestorResponse](#rill-v1-IsAncestorResponse) | Whether one revision is an ancestor of another. |



<a name="rill-v1-BatchCheckObjectsRequest"></a>

### BatchCheckObjectsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `objects` | [ObjectRef](#rill-v1-ObjectRef) | repeated |  |


<a name="rill-v1-BatchCheckObjectsResponse"></a>

### BatchCheckObjectsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `present` | [bool](#bool) | repeated | Parallel to `objects`. |


<a name="rill-v1-BatchReadFilesRequest"></a>

### BatchReadFilesRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `revision` | [string](#string) |  |  |
| `paths` | [string](#string) | repeated | At most 1,000. |
| `max_bytes` | [uint64](#uint64) |  | Per file, as in ReadFile; the whole response holds at most 16 MiB of content, the rest truncated. |


<a name="rill-v1-BatchReadFilesResponse"></a>

### BatchReadFilesResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `files` | [ReadFileResponse](#rill-v1-ReadFileResponse) | repeated | Parallel to `paths`; a missing path has no entry. |


<a name="rill-v1-BlameLine"></a>

### BlameLine



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `line_no` | [uint32](#uint32) |  |  |
| `version` | [string](#string) |  | `orgs/{org}/repos/{repo}/versions/{hash}`. |
| `author` | [Principal](#rill-v1-Principal) |  |  |
| `trunk_position` | [uint64](#uint64) | optional |  |
| `text` | [bytes](#bytes) |  |  |


<a name="rill-v1-BlameRequest"></a>

### BlameRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `revision` | [string](#string) |  |  |
| `path` | [string](#string) |  | Required. |


<a name="rill-v1-BlameResponse"></a>

### BlameResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `lines` | [BlameLine](#rill-v1-BlameLine) | repeated |  |


<a name="rill-v1-DiffRequest"></a>

### DiffRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `base` | [string](#string) |  | Required. |
| `target` | [string](#string) |  | Required. |
| `paths` | [string](#string) | repeated | Restrict to these paths or prefixes; empty means everything. |
| `format` | [DiffFormat](#rill-v1-DiffFormat) |  |  |
| `context_lines` | [uint32](#uint32) |  |  |
| `max_files` | [uint32](#uint32) |  | Most files to return; 0 means 1,000. |


<a name="rill-v1-DiffResponse"></a>

### DiffResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `files` | [FileDiff](#rill-v1-FileDiff) | repeated |  |
| `truncated` | [bool](#bool) |  | `max_files` or the 4 MiB budget cut the diff. |
| `read_grant` | [string](#string) |  | A read grant for the content on both sides. |


<a name="rill-v1-FindMergeBaseRequest"></a>

### FindMergeBaseRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `a` | [string](#string) |  | Required. |
| `b` | [string](#string) |  | Required. |


<a name="rill-v1-FindMergeBaseResponse"></a>

### FindMergeBaseResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `bases` | [string](#string) | repeated | Version names; empty when the two share no history. |


<a name="rill-v1-GetTreeRequest"></a>

### GetTreeRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `hash` | [bytes](#bytes) |  |  |
| `grant` | [string](#string) |  | A read grant naming the tree, from the listing that revealed it. |


<a name="rill-v1-GetTreeResponse"></a>

### GetTreeResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `tree` | [Tree](#rill-v1-Tree) |  |  |
| `read_grant` | [string](#string) |  |  |


<a name="rill-v1-GrepHit"></a>

### GrepHit



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `path` | [string](#string) |  |  |
| `line_no` | [uint32](#uint32) |  |  |
| `line` | [string](#string) |  |  |
| `before` | [string](#string) | repeated |  |
| `after` | [string](#string) | repeated |  |
| `byte_start` | [uint64](#uint64) |  |  |
| `byte_end` | [uint64](#uint64) |  |  |


<a name="rill-v1-GrepRequest"></a>

### GrepRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `revision` | [string](#string) |  |  |
| `pattern` | [string](#string) |  | Regular expression (Rust `regex` syntax). Required. |
| `case_insensitive` | [bool](#bool) |  |  |
| `path_globs` | [string](#string) | repeated | Glob filters on paths, e.g. `src/**/*.rs`; empty means all paths. |
| `max_results` | [uint32](#uint32) |  | Most hits; 0 means 5,000. |
| `context_lines` | [uint32](#uint32) |  |  |
| `literal` | [bool](#bool) |  | Match `pattern` as a literal string, not a regex. |
| `languages` | [string](#string) | repeated | Only files in these languages (`rust`, `python`, …); empty is all. |
| `min_seq` | [uint64](#uint64) | optional | Wait until trunk is indexed through this position (up to 30 s), so a search sees a landing the caller just made. |


<a name="rill-v1-GrepResponse"></a>

### GrepResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `hits` | [GrepHit](#rill-v1-GrepHit) | repeated | Best files first (definitions, source over vendored or test code, shallow paths, recent changes), then in line order within a file. |
| `truncated` | [bool](#bool) |  | More hits exist than were returned. |
| `indexed_through` | [uint64](#uint64) |  | Trunk is indexed through this position. |
| `files_matched` | [uint32](#uint32) |  | Files with a match, among those looked at. |
| `partial` | [bool](#bool) |  | The time budget ran out first: results are incomplete. |
| `directories` | [GrepResponse.DirectoriesEntry](#rill-v1-GrepResponse-DirectoriesEntry) | repeated | Matched files by top-level directory and by language. |
| `languages` | [GrepResponse.LanguagesEntry](#rill-v1-GrepResponse-LanguagesEntry) | repeated |  |


<a name="rill-v1-GrepResponse-DirectoriesEntry"></a>

### GrepResponse.DirectoriesEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key` | [string](#string) |  |  |
| `value` | [uint32](#uint32) |  |  |


<a name="rill-v1-GrepResponse-LanguagesEntry"></a>

### GrepResponse.LanguagesEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key` | [string](#string) |  |  |
| `value` | [uint32](#uint32) |  |  |


<a name="rill-v1-IsAncestorRequest"></a>

### IsAncestorRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `ancestor` | [string](#string) |  | Required. |
| `descendant` | [string](#string) |  | Required. |


<a name="rill-v1-IsAncestorResponse"></a>

### IsAncestorResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `is_ancestor` | [bool](#bool) |  |  |


<a name="rill-v1-ListDirectoryRequest"></a>

### ListDirectoryRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `revision` | [string](#string) |  |  |
| `path` | [string](#string) |  | Empty for the root. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListDirectoryResponse"></a>

### ListDirectoryResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `tree_hash` | [bytes](#bytes) |  |  |
| `entries` | [NamedTreeEntry](#rill-v1-NamedTreeEntry) | repeated | Entries the caller may read, and directories on the way to them. |
| `next_page_token` | [string](#string) |  |  |
| `read_grant` | [string](#string) |  | A read grant for the readable entries. |


<a name="rill-v1-ListHistoryRequest"></a>

### ListHistoryRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `revision` | [string](#string) |  | Where to start walking. |
| `path` | [string](#string) |  | Only versions touching this path or prefix. |
| `first_parent` | [bool](#bool) |  | Walk first parents only. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListHistoryResponse"></a>

### ListHistoryResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `versions` | [ChangeInfo](#rill-v1-ChangeInfo) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListPathHistoryRequest"></a>

### ListPathHistoryRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `revision` | [string](#string) |  |  |
| `path` | [string](#string) |  | Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListPathHistoryResponse"></a>

### ListPathHistoryResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `entries` | [PathHistoryEntry](#rill-v1-PathHistoryEntry) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ObjectRef"></a>

### ObjectRef



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `kind` | [ObjectKind](#rill-v1-ObjectKind) |  |  |
| `hash` | [bytes](#bytes) |  |  |


<a name="rill-v1-PathHistoryEntry"></a>

### PathHistoryEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `version` | [ChangeInfo](#rill-v1-ChangeInfo) |  |  |
| `path` | [string](#string) |  | The path as of this version (differs across renames). |
| `change` | [FileChange](#rill-v1-FileChange) |  |  |


<a name="rill-v1-ReadBlobRequest"></a>

### ReadBlobRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `content` | [ContentRef](#rill-v1-ContentRef) |  |  |
| `offset` | [uint64](#uint64) | optional | Byte range; the whole blob by default. |
| `length` | [uint64](#uint64) | optional |  |
| `grant` | [string](#string) |  | A read grant naming the content. Outside dev mode content is never served by hash alone. |


<a name="rill-v1-ReadBlobResponse"></a>

### ReadBlobResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `data` | [bytes](#bytes) |  |  |


<a name="rill-v1-ReadFileRequest"></a>

### ReadFileRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `revision` | [string](#string) |  |  |
| `path` | [string](#string) |  | Required. |
| `max_bytes` | [uint64](#uint64) |  | Content larger than this is not returned (`data` empty, `truncated`); read it with ReadBlob. 0 means 4 MiB, the most allowed. |


<a name="rill-v1-ReadFileResponse"></a>

### ReadFileResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `entry` | [TreeEntry](#rill-v1-TreeEntry) |  |  |
| `data` | [bytes](#bytes) |  |  |
| `truncated` | [bool](#bool) |  | The file is larger than `max_bytes`. |
| `read_grant` | [string](#string) |  | A read grant for the file's content. |


<a name="rill-v1-StatRequest"></a>

### StatRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `revision` | [string](#string) |  |  |
| `path` | [string](#string) |  |  |


<a name="rill-v1-StatResponse"></a>

### StatResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `entry` | [TreeEntry](#rill-v1-TreeEntry) | optional | Absent if the path does not exist. |
| `read_grant` | [string](#string) |  |  |


<a name="rill-v1-WriteBlobHeader"></a>

### WriteBlobHeader



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `size` | [uint64](#uint64) | optional | Total size, if known; lets the server pick chunking up front. |
| `expected_hash` | [bytes](#bytes) | optional | Expected content hash; the upload fails if it does not match. |


<a name="rill-v1-WriteBlobRequest"></a>

### WriteBlobRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `header` | [WriteBlobHeader](#rill-v1-WriteBlobHeader) |  |  |
| `data` | [bytes](#bytes) |  |  |


<a name="rill-v1-WriteBlobResponse"></a>

### WriteBlobResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `content` | [ContentRef](#rill-v1-ContentRef) |  |  |




## rill/v1/errors.proto





<a name="rill-v1-ErrorReason"></a>

### ErrorReason

Why a call failed, beyond its status code. Every error the server returns
carries a `google.rpc.ErrorInfo` detail whose `reason` is one of these
names without the `ERROR_REASON_` prefix (`ETAG_MISMATCH`) and whose
`domain` is `rill.dev`; `metadata` holds the values the message names.
Clients branch on the reason, never on the message text.

| Name | Number | Description |
| --- | --- | --- |
| `ERROR_REASON_UNSPECIFIED` | 0 |  |
| `ERROR_REASON_INVALID_FIELD` | 1 | INVALID_ARGUMENT: a field is malformed. A `google.rpc.BadRequest` detail names the field. |
| `ERROR_REASON_INVALID_NAME` | 2 | INVALID_ARGUMENT: a resource name does not parse. |
| `ERROR_REASON_INVALID_REVISION` | 3 | INVALID_ARGUMENT: a revision (`trunk@5`, a hash, a ref) does not parse. |
| `ERROR_REASON_INVALID_FILTER` | 4 | INVALID_ARGUMENT: a list filter does not parse or names an unknown field. |
| `ERROR_REASON_INVALID_PAGE_TOKEN` | 5 | INVALID_ARGUMENT: a page token is malformed or was issued for other parameters. |
| `ERROR_REASON_REQUEST_ID_REUSED` | 6 | INVALID_ARGUMENT: a `request_id` was already used for a different request. |
| `ERROR_REASON_TOO_LARGE` | 7 | RESOURCE_EXHAUSTED: a request or value is over a size limit. |
| `ERROR_REASON_UNAUTHENTICATED` | 10 | UNAUTHENTICATED: no token, or one that is malformed, expired or revoked. |
| `ERROR_REASON_PERMISSION_DENIED` | 11 | PERMISSION_DENIED: the caller lacks the relation the call needs. |
| `ERROR_REASON_CAPABILITY_DENIED` | 12 | PERMISSION_DENIED: the caller's token narrows what it may do, and this is outside it. |
| `ERROR_REASON_POLICY_DENIED` | 13 | FAILED_PRECONDITION: the repository's policy does not allow this yet. A `google.rpc.PreconditionFailure` detail lists what is missing. |
| `ERROR_REASON_NOT_FOUND` | 20 | NOT_FOUND: the named resource does not exist. |
| `ERROR_REASON_MISSING_OBJECTS` | 21 | FAILED_PRECONDITION: objects the request refers to were never uploaded; upload them and retry. |
| `ERROR_REASON_ALREADY_EXISTS` | 22 | ALREADY_EXISTS: the resource to create exists. |
| `ERROR_REASON_ETAG_MISMATCH` | 30 | ABORTED: the `etag` given is not the resource's current one; read it again and retry. |
| `ERROR_REASON_REF_MOVED` | 31 | ABORTED: a ref is no longer at the target the request expected. |
| `ERROR_REASON_CONTENDED` | 32 | ABORTED: another writer won a race; retrying is safe. |
| `ERROR_REASON_VERSION_SUPERSEDED` | 33 | FAILED_PRECONDITION: the change has a newer version than the one named. |
| `ERROR_REASON_CONFLICTS_UNRESOLVED` | 34 | FAILED_PRECONDITION: the version holds unresolved conflicts. |
| `ERROR_REASON_INVALID_STATE` | 35 | FAILED_PRECONDITION: the resource is not in a state that allows this (a landed change, a resolved thread). |
| `ERROR_REASON_REDACTED` | 36 | FAILED_PRECONDITION: the content was redacted. |
| `ERROR_REASON_RATE_LIMITED` | 40 | RESOURCE_EXHAUSTED: the caller is over its request rate. A `google.rpc.RetryInfo` detail says when to retry. |
| `ERROR_REASON_QUEUE_FULL` | 41 | RESOURCE_EXHAUSTED: the landing queue is full. Retry later. |
| `ERROR_REASON_BUDGET_EXHAUSTED` | 42 | RESOURCE_EXHAUSTED: an agent or sponsor budget is spent. |
| `ERROR_REASON_UNAVAILABLE` | 50 | UNAVAILABLE: this server cannot answer now (draining, a store outage, a request still running elsewhere); retry, possibly elsewhere. |
| `ERROR_REASON_UNIMPLEMENTED` | 51 | UNIMPLEMENTED: the server does not support this. |
| `ERROR_REASON_TIMEOUT` | 52 | DEADLINE_EXCEEDED: the call ran out of time. |
| `ERROR_REASON_INTERNAL` | 53 | INTERNAL or DATA_LOSS: a server fault; the message is for operators. |



## rill/v1/events.proto

The repository event log: one event per committed state change.

<a name="rill-v1-EventService"></a>

### EventService

The repository event log, live and replayed.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `WatchEvents` | [WatchEventsRequest](#rill-v1-WatchEventsRequest) | [WatchEventsResponse](#rill-v1-WatchEventsResponse) stream | Every committed state change, filtered and resumable by cursor. |



<a name="rill-v1-CommentedEvent"></a>

### CommentedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change_id` | [bytes](#bytes) |  |  |
| `thread_id` | [string](#string) |  |  |
| `new_thread` | [bool](#bool) |  |  |


<a name="rill-v1-Event"></a>

### Event



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `actor` | [Principal](#rill-v1-Principal) |  |  |
| `at` | [int64](#int64) |  |  |
| `repo_created` | [RepoCreatedEvent](#rill-v1-RepoCreatedEvent) |  |  |
| `ref_moved` | [RefMovedEvent](#rill-v1-RefMovedEvent) |  |  |
| `review_submitted` | [ReviewSubmittedEvent](#rill-v1-ReviewSubmittedEvent) |  |  |
| `voted` | [VotedEvent](#rill-v1-VotedEvent) |  |  |
| `commented` | [CommentedEvent](#rill-v1-CommentedEvent) |  |  |
| `thread_resolved` | [ThreadResolvedEvent](#rill-v1-ThreadResolvedEvent) |  |  |
| `stack_set` | [StackSetEvent](#rill-v1-StackSetEvent) |  |  |
| `workspace_changed` | [WorkspaceChangedEvent](#rill-v1-WorkspaceChangedEvent) |  |  |
| `workspace_deleted` | [WorkspaceDeletedEvent](#rill-v1-WorkspaceDeletedEvent) |  |  |
| `intent_changed` | [IntentChangedEvent](#rill-v1-IntentChangedEvent) |  |  |
| `relation_changed` | [RelationChangedEvent](#rill-v1-RelationChangedEvent) |  |  |
| `policy_changed` | [PolicyChangedEvent](#rill-v1-PolicyChangedEvent) |  |  |
| `version_stored` | [VersionStoredEvent](#rill-v1-VersionStoredEvent) |  |  |
| `status_changed` | [StatusChangedEvent](#rill-v1-StatusChangedEvent) |  |  |
| `landed` | [LandedEvent](#rill-v1-LandedEvent) |  |  |
| `land_requested` | [LandRequestedEvent](#rill-v1-LandRequestedEvent) |  |  |
| `land_status_changed` | [LandStatusChangedEvent](#rill-v1-LandStatusChangedEvent) |  |  |
| `quarantine_changed` | [QuarantineChangedEvent](#rill-v1-QuarantineChangedEvent) |  |  |
| `incident_changed` | [IncidentChangedEvent](#rill-v1-IncidentChangedEvent) |  |  |
| `trunk_health_changed` | [TrunkHealthChangedEvent](#rill-v1-TrunkHealthChangedEvent) |  |  |
| `reverts_requested` | [RevertsRequestedEvent](#rill-v1-RevertsRequestedEvent) |  |  |


<a name="rill-v1-IncidentChangedEvent"></a>

### IncidentChangedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `incident` | [string](#string) |  |  |
| `culprit` | [uint64](#uint64) |  |  |
| `state` | [string](#string) |  |  |


<a name="rill-v1-IntentChangedEvent"></a>

### IntentChangedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent_id` | [bytes](#bytes) |  |  |


<a name="rill-v1-LandRequestedEvent"></a>

### LandRequestedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `ticket` | [string](#string) |  |  |
| `change_id` | [bytes](#bytes) |  |  |
| `lane` | [string](#string) |  | "emergency", "human", "agent" or "bulk". |


<a name="rill-v1-LandStatusChangedEvent"></a>

### LandStatusChangedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `ticket` | [string](#string) |  |  |
| `change_id` | [bytes](#bytes) |  |  |
| `status` | [string](#string) |  | "queued", "verifying", "landing", "landed", "failed", "conflict", "returned" or "ejected". |
| `reason` | [string](#string) | optional | Why it failed, was returned or ejected. |
| `seq` | [uint64](#uint64) | optional | Where it landed. |
| `conflict_paths` | [string](#string) | repeated | The paths a conflicting rebase stopped on. |


<a name="rill-v1-LandedEvent"></a>

### LandedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `seq` | [uint64](#uint64) |  |  |
| `version` | [bytes](#bytes) |  |  |
| `change_id` | [bytes](#bytes) |  |  |


<a name="rill-v1-PolicyChangedEvent"></a>

### PolicyChangedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  |  |


<a name="rill-v1-QuarantineChangedEvent"></a>

### QuarantineChangedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `target` | [string](#string) |  |  |
| `quarantined` | [bool](#bool) |  |  |


<a name="rill-v1-RefMovedEvent"></a>

### RefMovedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  |  |
| `old` | [bytes](#bytes) | optional |  |
| `new` | [bytes](#bytes) | optional |  |


<a name="rill-v1-RelationChangedEvent"></a>

### RelationChangedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `object` | [string](#string) |  |  |
| `relation` | [string](#string) |  |  |
| `subject` | [string](#string) |  |  |
| `added` | [bool](#bool) |  |  |


<a name="rill-v1-RepoCreatedEvent"></a>

### RepoCreatedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  |  |


<a name="rill-v1-RevertsRequestedEvent"></a>

### RevertsRequestedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `selector` | [string](#string) |  |  |
| `count` | [uint32](#uint32) |  |  |


<a name="rill-v1-ReviewSubmittedEvent"></a>

### ReviewSubmittedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change_id` | [bytes](#bytes) |  |  |
| `version` | [bytes](#bytes) |  |  |


<a name="rill-v1-StackSetEvent"></a>

### StackSetEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change_id` | [bytes](#bytes) |  |  |
| `stack` | [bytes](#bytes) | repeated |  |


<a name="rill-v1-StatusChangedEvent"></a>

### StatusChangedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change_id` | [bytes](#bytes) |  |  |
| `from` | [ReviewStatus](#rill-v1-ReviewStatus) |  |  |
| `to` | [ReviewStatus](#rill-v1-ReviewStatus) |  |  |


<a name="rill-v1-ThreadResolvedEvent"></a>

### ThreadResolvedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change_id` | [bytes](#bytes) |  |  |
| `thread_id` | [string](#string) |  |  |
| `resolved` | [bool](#bool) |  |  |


<a name="rill-v1-TrunkHealthChangedEvent"></a>

### TrunkHealthChangedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `state` | [string](#string) |  |  |
| `frozen` | [bool](#bool) |  |  |


<a name="rill-v1-VersionStoredEvent"></a>

### VersionStoredEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `version` | [bytes](#bytes) |  |  |
| `change_id` | [bytes](#bytes) |  |  |


<a name="rill-v1-VotedEvent"></a>

### VotedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change_id` | [bytes](#bytes) |  |  |
| `label` | [string](#string) |  |  |
| `value` | [int32](#int32) |  |  |
| `version` | [bytes](#bytes) |  |  |


<a name="rill-v1-WatchEventsRequest"></a>

### WatchEventsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `after` | [bytes](#bytes) | optional | Absent: start after the newest event, so only new events are sent. Empty: replay from the oldest retained event. Otherwise resume after this cursor, taken from an earlier response. |
| `kinds` | [string](#string) | repeated | Event kinds to send ("ref_moved", "landed", ...); empty sends all. |
| `change_id` | [bytes](#bytes) | optional | Only events about this change. |
| `ref_prefix` | [string](#string) | optional | Only ref moves of refs starting with this prefix. |


<a name="rill-v1-WatchEventsResponse"></a>

### WatchEventsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `cursor` | [bytes](#bytes) |  |  |
| `event` | [Event](#rill-v1-Event) |  |  |


<a name="rill-v1-WorkspaceChangedEvent"></a>

### WorkspaceChangedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `workspace_id` | [bytes](#bytes) |  |  |
| `operation_id` | [bytes](#bytes) |  |  |


<a name="rill-v1-WorkspaceDeletedEvent"></a>

### WorkspaceDeletedEvent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `workspace_id` | [bytes](#bytes) |  |  |




## rill/v1/identity.proto

Identity: who the caller is, tokens and their keys, and each
organization's identity providers, users, agents, workload issuers and
SCIM credentials.

Organization resources: `orgs/{org}/identityProviders/{provider}`,
`orgs/{org}/users/{user}`, `orgs/{org}/agents/{agent}`,
`orgs/{org}/workloadIssuers/{issuer}`,
`orgs/{org}/scimCredentials/{credential}`.

<a name="rill-v1-IdentityService"></a>

### IdentityService

Identities, tokens and the organization's sign-in setup.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `GetCaller` | [GetCallerRequest](#rill-v1-GetCallerRequest) | [GetCallerResponse](#rill-v1-GetCallerResponse) | The caller: principal, teams, capabilities and token. |
| `IssueToken` | [IssueTokenRequest](#rill-v1-IssueTokenRequest) | [IssueTokenResponse](#rill-v1-IssueTokenResponse) | Mint a capability token attenuated from the caller's. |
| `RevokeToken` | [RevokeTokenRequest](#rill-v1-RevokeTokenRequest) | [RevokeTokenResponse](#rill-v1-RevokeTokenResponse) | Revoke a token (and every token attenuated from it). Callers may revoke a token they present; administrators any token id. |
| `RevokeSession` | [RevokeSessionRequest](#rill-v1-RevokeSessionRequest) | [RevokeSessionResponse](#rill-v1-RevokeSessionResponse) | End a session; every token issued under it is refused from then on. |
| `RotateTokenKeys` | [RotateTokenKeysRequest](#rill-v1-RotateTokenKeysRequest) | [RotateTokenKeysResponse](#rill-v1-RotateTokenKeysResponse) | Store a new signing key now; it activates after the activation delay. |
| `ListTokenKeys` | [ListTokenKeysRequest](#rill-v1-ListTokenKeysRequest) | [ListTokenKeysResponse](#rill-v1-ListTokenKeysResponse) | The token signing keys, active and retired. |
| `GetIdentityProvider` | [GetIdentityProviderRequest](#rill-v1-GetIdentityProviderRequest) | [GetIdentityProviderResponse](#rill-v1-GetIdentityProviderResponse) | One OpenID Connect or SAML provider; org administrators. |
| `ListIdentityProviders` | [ListIdentityProvidersRequest](#rill-v1-ListIdentityProvidersRequest) | [ListIdentityProvidersResponse](#rill-v1-ListIdentityProvidersResponse) | An organization's identity providers; org administrators. |
| `UpdateIdentityProvider` | [UpdateIdentityProviderRequest](#rill-v1-UpdateIdentityProviderRequest) | [UpdateIdentityProviderResponse](#rill-v1-UpdateIdentityProviderResponse) | Replace a provider, or with `allow_missing` create it; org administrators. |
| `DeleteIdentityProvider` | [DeleteIdentityProviderRequest](#rill-v1-DeleteIdentityProviderRequest) | [DeleteIdentityProviderResponse](#rill-v1-DeleteIdentityProviderResponse) | Remove a provider; org administrators. |
| `ListUsers` | [ListUsersRequest](#rill-v1-ListUsersRequest) | [ListUsersResponse](#rill-v1-ListUsersResponse) | An organization's users; org administrators. |
| `UpdateUser` | [UpdateUserRequest](#rill-v1-UpdateUserRequest) | [UpdateUserResponse](#rill-v1-UpdateUserResponse) | Suspend or reactivate a user, or set their roles. Suspension refuses every token the user holds at once. |
| `CreateAgent` | [CreateAgentRequest](#rill-v1-CreateAgentRequest) | [CreateAgentResponse](#rill-v1-CreateAgentResponse) | Register an agent: org administrators, or a user for agents they sponsor. |
| `GetAgent` | [GetAgentRequest](#rill-v1-GetAgentRequest) | [GetAgentResponse](#rill-v1-GetAgentResponse) | One agent. |
| `ListAgents` | [ListAgentsRequest](#rill-v1-ListAgentsRequest) | [ListAgentsResponse](#rill-v1-ListAgentsResponse) | An organization's agents. |
| `UpdateAgent` | [UpdateAgentRequest](#rill-v1-UpdateAgentRequest) | [UpdateAgentResponse](#rill-v1-UpdateAgentResponse) | Suspend or reactivate an agent, or replace its registration. |
| `ListWorkloadIssuers` | [ListWorkloadIssuersRequest](#rill-v1-ListWorkloadIssuersRequest) | [ListWorkloadIssuersResponse](#rill-v1-ListWorkloadIssuersResponse) | Issuers whose workload JWTs may be exchanged for agent tokens; org administrators. |
| `UpdateWorkloadIssuer` | [UpdateWorkloadIssuerRequest](#rill-v1-UpdateWorkloadIssuerRequest) | [UpdateWorkloadIssuerResponse](#rill-v1-UpdateWorkloadIssuerResponse) | Replace an issuer, or with `allow_missing` create it. |
| `DeleteWorkloadIssuer` | [DeleteWorkloadIssuerRequest](#rill-v1-DeleteWorkloadIssuerRequest) | [DeleteWorkloadIssuerResponse](#rill-v1-DeleteWorkloadIssuerResponse) | Remove an issuer. |
| `KillSwitch` | [KillSwitchRequest](#rill-v1-KillSwitchRequest) | [KillSwitchResponse](#rill-v1-KillSwitchResponse) | Stop an agent, a session, everything a sponsor's agents do, or every agent running as a model; org administrators. Tokens are refused at once, and the lander ejects their queued work. |
| `CreateScimCredential` | [CreateScimCredentialRequest](#rill-v1-CreateScimCredentialRequest) | [CreateScimCredentialResponse](#rill-v1-CreateScimCredentialResponse) | Bearer credentials a provisioning system calls the org's SCIM endpoint (`/scim/v2/<org>`) with; org administrators. The secret is returned once. |
| `ListScimCredentials` | [ListScimCredentialsRequest](#rill-v1-ListScimCredentialsRequest) | [ListScimCredentialsResponse](#rill-v1-ListScimCredentialsResponse) | An organization's SCIM credentials, revoked ones included. |
| `DeleteScimCredential` | [DeleteScimCredentialRequest](#rill-v1-DeleteScimCredentialRequest) | [DeleteScimCredentialResponse](#rill-v1-DeleteScimCredentialResponse) | Revoke a SCIM credential; it stays listed as revoked. |



<a name="rill-v1-Agent"></a>

### Agent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/agents/{agent}`. Output only. |
| `etag` | [string](#string) |  | Output only. |
| `spec` | [AgentSpec](#rill-v1-AgentSpec) |  |  |
| `principal` | [Principal](#rill-v1-Principal) |  |  |
| `status` | [string](#string) |  | `active` or `suspended`. |
| `created_by` | [Principal](#rill-v1-Principal) |  |  |
| `created_at` | [int64](#int64) |  |  |
| `updated_at` | [int64](#int64) |  |  |


<a name="rill-v1-AgentBudgets"></a>

### AgentBudgets



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `versions_per_day` | [uint64](#uint64) | optional |  |
| `lands_per_day` | [uint64](#uint64) | optional |  |
| `requests_per_minute` | [uint64](#uint64) | optional |  |
| `executions_per_day` | [uint64](#uint64) | optional |  |


<a name="rill-v1-AgentSpec"></a>

### AgentSpec



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `org` | [string](#string) |  |  |
| `name` | [string](#string) |  |  |
| `sponsor` | [string](#string) |  | `user:<name>` or `team:<name>`. |
| `purpose` | [string](#string) |  |  |
| `repos` | [string](#string) | repeated | Repository names in the org; `*` for all. |
| `paths` | [string](#string) | repeated | Path prefixes; empty for every path. |
| `models` | [string](#string) | repeated | Models it may run as; empty for any. |
| `budgets` | [AgentBudgets](#rill-v1-AgentBudgets) |  |  |
| `workloads` | [WorkloadBinding](#rill-v1-WorkloadBinding) | repeated |  |
| `expires_at` | [int64](#int64) | optional |  |
| `reviewer` | [ReviewerProfile](#rill-v1-ReviewerProfile) |  | Set when the agent reviews changes. |


<a name="rill-v1-Capability"></a>

### Capability

A capability the caller's token grants, as a human-readable summary.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `action` | [string](#string) |  | Action name, e.g. `repo.write`, `review.vote`, `lander.land`. |
| `scope` | [string](#string) |  | Object pattern the action applies to, e.g. `repo:main/*`. |
| `expires_at` | [int64](#int64) | optional |  |


<a name="rill-v1-CreateAgentRequest"></a>

### CreateAgentRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `agent` | [AgentSpec](#rill-v1-AgentSpec) |  | The registration; its org and name are the new agent's. Required. |


<a name="rill-v1-CreateAgentResponse"></a>

### CreateAgentResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `agent` | [Agent](#rill-v1-Agent) |  |  |


<a name="rill-v1-CreateScimCredentialRequest"></a>

### CreateScimCredentialRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |


<a name="rill-v1-CreateScimCredentialResponse"></a>

### CreateScimCredentialResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `credential` | [ScimCredential](#rill-v1-ScimCredential) |  |  |
| `token` | [string](#string) |  | The bearer token to give the provisioning system; not stored. |


<a name="rill-v1-DeleteIdentityProviderRequest"></a>

### DeleteIdentityProviderRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `orgs/{org}/identityProviders/{provider}`. Required. |


<a name="rill-v1-DeleteIdentityProviderResponse"></a>

### DeleteIdentityProviderResponse




<a name="rill-v1-DeleteScimCredentialRequest"></a>

### DeleteScimCredentialRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `orgs/{org}/scimCredentials/{credential}`. Required. |


<a name="rill-v1-DeleteScimCredentialResponse"></a>

### DeleteScimCredentialResponse




<a name="rill-v1-DeleteWorkloadIssuerRequest"></a>

### DeleteWorkloadIssuerRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `orgs/{org}/workloadIssuers/{issuer}`. Required. |


<a name="rill-v1-DeleteWorkloadIssuerResponse"></a>

### DeleteWorkloadIssuerResponse




<a name="rill-v1-GetAgentRequest"></a>

### GetAgentRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/agents/{agent}`. Required. |


<a name="rill-v1-GetAgentResponse"></a>

### GetAgentResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `agent` | [Agent](#rill-v1-Agent) |  |  |


<a name="rill-v1-GetCallerRequest"></a>

### GetCallerRequest




<a name="rill-v1-GetCallerResponse"></a>

### GetCallerResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `principal` | [Principal](#rill-v1-Principal) |  |  |
| `teams` | [string](#string) | repeated |  |
| `capabilities` | [Capability](#rill-v1-Capability) | repeated |  |
| `token_expires_at` | [int64](#int64) | optional |  |
| `token_id` | [string](#string) | optional | The caller's token id (`tk_…`), session (`ss_…`) and organization, when the caller presented an issued token. |
| `session` | [string](#string) | optional |  |
| `org` | [string](#string) | optional |  |
| `sponsor` | [string](#string) | optional |  |
| `model` | [string](#string) | optional |  |


<a name="rill-v1-GetIdentityProviderRequest"></a>

### GetIdentityProviderRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/identityProviders/{provider}`. Required. |


<a name="rill-v1-GetIdentityProviderResponse"></a>

### GetIdentityProviderResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `provider` | [IdentityProviderInfo](#rill-v1-IdentityProviderInfo) |  |  |


<a name="rill-v1-IdentityProviderInfo"></a>

### IdentityProviderInfo



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `resource_name` | [string](#string) |  | `orgs/{org}/identityProviders/{provider}`. |
| `org` | [string](#string) |  |  |
| `name` | [string](#string) |  |  |
| `issuer` | [string](#string) |  |  |
| `client_id` | [string](#string) |  |  |
| `has_client_secret` | [bool](#bool) |  |  |
| `cli_client_id` | [string](#string) | optional |  |
| `scopes` | [string](#string) | repeated |  |
| `username_claim` | [string](#string) |  |  |
| `groups_claim` | [string](#string) | optional |  |
| `auto_provision` | [bool](#bool) |  |  |
| `created_at` | [int64](#int64) |  |  |
| `saml` | [bool](#bool) |  |  |
| `saml_sp_entity_id` | [string](#string) | optional |  |


<a name="rill-v1-IdentityProviderSpec"></a>

### IdentityProviderSpec



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `org` | [string](#string) |  |  |
| `name` | [string](#string) |  |  |
| `issuer` | [string](#string) |  |  |
| `client_id` | [string](#string) |  |  |
| `client_secret` | [string](#string) | optional | Wrapped by the instance's key service before it is stored; never returned. |
| `cli_client_id` | [string](#string) | optional |  |
| `scopes` | [string](#string) | repeated |  |
| `username_claim` | [string](#string) | optional | Defaults to `preferred_username`. |
| `groups_claim` | [string](#string) | optional |  |
| `auto_provision` | [bool](#bool) |  |  |
| `saml_metadata` | [string](#string) | optional | A SAML 2.0 provider instead of OIDC: the IdP's metadata XML. `issuer` is then taken from it and the client fields are ignored; the username is the NameID unless `username_claim` names an attribute, and groups come from the attribute `groups_claim` names. |
| `saml_sp_entity_id` | [string](#string) | optional | Rill's entity id at the IdP; defaults to `<public_url>/auth/saml/<org>/<name>`. |


<a name="rill-v1-IssueTokenRequest"></a>

### IssueTokenRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `capabilities` | [Capability](#rill-v1-Capability) | repeated | Capabilities to attenuate the caller's token to; must be a subset. |
| `expires_at` | [int64](#int64) | optional |  |
| `for_principal` | [Principal](#rill-v1-Principal) | optional | Principal the token acts as; defaults to the caller. Used to mint agent identities sponsored by the caller. |


<a name="rill-v1-IssueTokenResponse"></a>

### IssueTokenResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `token` | [string](#string) |  |  |
| `expires_at` | [int64](#int64) |  |  |


<a name="rill-v1-KillSwitchRequest"></a>

### KillSwitchRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `agent` | [string](#string) |  |  |
| `session` | [string](#string) |  | `ss_…` |
| `sponsor` | [string](#string) |  | `user:<name>` or `team:<name>`. |
| `model` | [string](#string) |  |  |
| `reason` | [string](#string) |  |  |


<a name="rill-v1-KillSwitchResponse"></a>

### KillSwitchResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `agents_suspended` | [string](#string) | repeated |  |
| `sessions_revoked` | [string](#string) | repeated |  |
| `models_blocked` | [string](#string) | repeated |  |


<a name="rill-v1-ListAgentsRequest"></a>

### ListAgentsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `filter` | [string](#string) |  | On `status` (`active`, `suspended`) and `sponsor`. |


<a name="rill-v1-ListAgentsResponse"></a>

### ListAgentsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `agents` | [Agent](#rill-v1-Agent) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListIdentityProvidersRequest"></a>

### ListIdentityProvidersRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListIdentityProvidersResponse"></a>

### ListIdentityProvidersResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `providers` | [IdentityProviderInfo](#rill-v1-IdentityProviderInfo) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListScimCredentialsRequest"></a>

### ListScimCredentialsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListScimCredentialsResponse"></a>

### ListScimCredentialsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `credentials` | [ScimCredential](#rill-v1-ScimCredential) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListTokenKeysRequest"></a>

### ListTokenKeysRequest




<a name="rill-v1-ListTokenKeysResponse"></a>

### ListTokenKeysResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `keys` | [TokenKey](#rill-v1-TokenKey) | repeated |  |


<a name="rill-v1-ListUsersRequest"></a>

### ListUsersRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `filter` | [string](#string) |  | On `status` (`active`, `suspended`), `roles` and `teams`. |


<a name="rill-v1-ListUsersResponse"></a>

### ListUsersResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `users` | [User](#rill-v1-User) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListWorkloadIssuersRequest"></a>

### ListWorkloadIssuersRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListWorkloadIssuersResponse"></a>

### ListWorkloadIssuersResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `issuers` | [WorkloadIssuerSpec](#rill-v1-WorkloadIssuerSpec) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ReviewerProfile"></a>

### ReviewerProfile

How an agent reviews.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `model_family` | [string](#string) |  | The model's family (`claude`, …); a quorum needs different families or prompts. |
| `model` | [string](#string) |  |  |
| `prompt` | [string](#string) |  | Its prompt's repository path; default `.rill/reviewers/<name>/prompt.md`. |
| `max_vote` | [int32](#int32) |  | The largest `Review` vote it may cast, either way (1 or 2). |
| `runs_per_day` | [uint64](#uint64) | optional |  |


<a name="rill-v1-RevokeSessionRequest"></a>

### RevokeSessionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `session` | [string](#string) |  | `ss_…` |


<a name="rill-v1-RevokeSessionResponse"></a>

### RevokeSessionResponse




<a name="rill-v1-RevokeTokenRequest"></a>

### RevokeTokenRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `token` | [string](#string) |  | The token itself: revokes its token id. |
| `token_id` | [string](#string) |  | `tk_…`; administrators only. |
| `reason` | [string](#string) |  |  |


<a name="rill-v1-RevokeTokenResponse"></a>

### RevokeTokenResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `token_id` | [string](#string) |  |  |


<a name="rill-v1-RotateTokenKeysRequest"></a>

### RotateTokenKeysRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |


<a name="rill-v1-RotateTokenKeysResponse"></a>

### RotateTokenKeysResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key_id` | [uint32](#uint32) | optional | Absent when another server rotated at the same moment. |
| `active_from` | [int64](#int64) |  |  |


<a name="rill-v1-ScimCredential"></a>

### ScimCredential



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/scimCredentials/{credential}`. |
| `org` | [string](#string) |  |  |
| `id` | [string](#string) |  |  |
| `created_by` | [Principal](#rill-v1-Principal) |  |  |
| `created_at` | [int64](#int64) |  |  |
| `revoked_at` | [int64](#int64) | optional |  |


<a name="rill-v1-TokenKey"></a>

### TokenKey



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `id` | [uint32](#uint32) |  |  |
| `public_key` | [string](#string) |  | Hex compressed P-256 public key. |
| `backend` | [string](#string) |  | The key service that wraps it and the wrapping key. |
| `key_ref` | [string](#string) |  |  |
| `created_at` | [int64](#int64) |  |  |
| `active_from` | [int64](#int64) |  |  |
| `not_after` | [int64](#int64) | optional |  |


<a name="rill-v1-UpdateAgentRequest"></a>

### UpdateAgentRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `agent` | [Agent](#rill-v1-Agent) |  | The agent, by `name`, with the fields to change. Required. |
| `update_mask` | [google.protobuf.FieldMask](#google-protobuf-FieldMask) |  | `status` (`active` or `suspended`) and/or `spec`, which replaces the registration's sponsor, scope, models, budgets, bindings and expiry; its org and name must match. Required. |
| `etag` | [string](#string) |  | The etag read; empty means no condition. |


<a name="rill-v1-UpdateAgentResponse"></a>

### UpdateAgentResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `agent` | [Agent](#rill-v1-Agent) |  |  |


<a name="rill-v1-UpdateIdentityProviderRequest"></a>

### UpdateIdentityProviderRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `provider` | [IdentityProviderSpec](#rill-v1-IdentityProviderSpec) |  | The provider; its org and name say which. Required. |
| `allow_missing` | [bool](#bool) |  | Create it when absent. |


<a name="rill-v1-UpdateIdentityProviderResponse"></a>

### UpdateIdentityProviderResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `provider` | [IdentityProviderInfo](#rill-v1-IdentityProviderInfo) |  |  |


<a name="rill-v1-UpdateUserRequest"></a>

### UpdateUserRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `user` | [User](#rill-v1-User) |  | The user, by `name`, with the fields to change. Required. |
| `update_mask` | [google.protobuf.FieldMask](#google-protobuf-FieldMask) |  | `status` (`active` or `suspended`) and/or `roles`. Required. |
| `etag` | [string](#string) |  | The etag read; empty means no condition. |


<a name="rill-v1-UpdateUserResponse"></a>

### UpdateUserResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `user` | [User](#rill-v1-User) |  |  |


<a name="rill-v1-UpdateWorkloadIssuerRequest"></a>

### UpdateWorkloadIssuerRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `issuer` | [WorkloadIssuerSpec](#rill-v1-WorkloadIssuerSpec) |  | The issuer; its org and name say which. Required. |
| `allow_missing` | [bool](#bool) |  | Create it when absent. |


<a name="rill-v1-UpdateWorkloadIssuerResponse"></a>

### UpdateWorkloadIssuerResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `issuer` | [WorkloadIssuerSpec](#rill-v1-WorkloadIssuerSpec) |  |  |


<a name="rill-v1-User"></a>

### User



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/users/{user}`, by user name. Output only. |
| `etag` | [string](#string) |  | Output only. |
| `org` | [string](#string) |  |  |
| `principal` | [Principal](#rill-v1-Principal) |  |  |
| `email` | [string](#string) | optional |  |
| `display_name` | [string](#string) | optional |  |
| `status` | [string](#string) |  | `active` or `suspended`. |
| `roles` | [string](#string) | repeated | `admin`. |
| `teams` | [string](#string) | repeated |  |
| `external` | [string](#string) | repeated | `issuer#subject` for each linked identity. |
| `created_at` | [int64](#int64) |  |  |
| `updated_at` | [int64](#int64) |  |  |


<a name="rill-v1-WorkloadBinding"></a>

### WorkloadBinding



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `issuer` | [string](#string) |  |  |
| `subject` | [string](#string) |  | The `sub` claim; a trailing `*` matches any suffix. |
| `claims` | [WorkloadBinding.ClaimsEntry](#rill-v1-WorkloadBinding-ClaimsEntry) | repeated | Further claims that must equal these values. |


<a name="rill-v1-WorkloadBinding-ClaimsEntry"></a>

### WorkloadBinding.ClaimsEntry



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key` | [string](#string) |  |  |
| `value` | [string](#string) |  |  |


<a name="rill-v1-WorkloadIssuerSpec"></a>

### WorkloadIssuerSpec



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `resource_name` | [string](#string) |  | `orgs/{org}/workloadIssuers/{issuer}`. Output only. |
| `org` | [string](#string) |  |  |
| `name` | [string](#string) |  |  |
| `issuer` | [string](#string) |  |  |
| `discovery` | [bool](#bool) |  | OIDC discovery at the issuer. |
| `jwks_uri` | [string](#string) |  |  |
| `jwks` | [string](#string) |  | A JWKS document. |
| `audiences` | [string](#string) | repeated |  |




## rill/v1/intent.proto

Intents: the "why" behind changes.

An intent is `orgs/{org}/repos/{repo}/intents/{intent}`.

<a name="rill-v1-IntentService"></a>

### IntentService

Intents and the changes linked to them.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `CreateIntent` | [CreateIntentRequest](#rill-v1-CreateIntentRequest) | [CreateIntentResponse](#rill-v1-CreateIntentResponse) | Record a new intent. |
| `GetIntent` | [GetIntentRequest](#rill-v1-GetIntentRequest) | [GetIntentResponse](#rill-v1-GetIntentResponse) | One intent. |
| `UpdateIntent` | [UpdateIntentRequest](#rill-v1-UpdateIntentRequest) | [UpdateIntentResponse](#rill-v1-UpdateIntentResponse) | Change an intent's title, body, status, parent, risk override or scope. Changing the scope starts a new revision, which needs approving. |
| `ListIntents` | [ListIntentsRequest](#rill-v1-ListIntentsRequest) | [ListIntentsResponse](#rill-v1-ListIntentsResponse) | A repository's intents, most recently updated first. |
| `LinkChange` | [LinkChangeRequest](#rill-v1-LinkChangeRequest) | [LinkChangeResponse](#rill-v1-LinkChangeResponse) | Link a change to an intent, on both the intent and the change's review. |
| `FindSimilarIntents` | [FindSimilarIntentsRequest](#rill-v1-FindSimilarIntentsRequest) | [FindSimilarIntentsResponse](#rill-v1-FindSimilarIntentsResponse) | Intents whose words overlap a title and body, best first. |
| `ApproveIntent` | [ApproveIntentRequest](#rill-v1-ApproveIntentRequest) | [ApproveIntentResponse](#rill-v1-ApproveIntentResponse) | Approve the intent's current scope revision, for the scope directories the caller owns. |
| `RevokeIntentApproval` | [RevokeIntentApprovalRequest](#rill-v1-RevokeIntentApprovalRequest) | [RevokeIntentApprovalResponse](#rill-v1-RevokeIntentApprovalResponse) | Withdraw the caller's approval. |
| `ExplainIntentScope` | [ExplainIntentScopeRequest](#rill-v1-ExplainIntentScopeRequest) | [ExplainIntentScopeResponse](#rill-v1-ExplainIntentScopeResponse) | Whether a change's intent is approved and the change inside its scope. |



<a name="rill-v1-ApproveIntentRequest"></a>

### ApproveIntentRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/intents/{intent}`. Required. |
| `revision` | [uint32](#uint32) |  | The revision being approved; refused when it is not current. |


<a name="rill-v1-ApproveIntentResponse"></a>

### ApproveIntentResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent` | [Intent](#rill-v1-Intent) |  |  |


<a name="rill-v1-CreateIntentRequest"></a>

### CreateIntentRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `title` | [string](#string) |  | Required. |
| `body` | [string](#string) |  |  |
| `kind` | [IntentKind](#rill-v1-IntentKind) |  |  |
| `parent_intent` | [string](#string) |  | The parent intent, `…/intents/{intent}`. |
| `risk_override` | [RiskClass](#rill-v1-RiskClass) | optional |  |
| `scope` | [IntentScope](#rill-v1-IntentScope) |  | The scope to approve; unset for none. |


<a name="rill-v1-CreateIntentResponse"></a>

### CreateIntentResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent` | [Intent](#rill-v1-Intent) |  |  |


<a name="rill-v1-ExplainIntentScopeRequest"></a>

### ExplainIntentScopeRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |


<a name="rill-v1-ExplainIntentScopeResponse"></a>

### ExplainIntentScopeResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent` | [string](#string) |  | The linked intent, `…/intents/{intent}`; empty when there is none. |
| `approved` | [bool](#bool) |  |  |
| `in_scope` | [bool](#bool) |  |  |
| `reasons` | [string](#string) | repeated | Why the change is not in scope. |
| `missing_approvals` | [string](#string) | repeated | Scope directories still short of approvals. |


<a name="rill-v1-FindSimilarIntentsRequest"></a>

### FindSimilarIntentsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `title` | [string](#string) |  |  |
| `body` | [string](#string) |  |  |
| `max_results` | [int32](#int32) |  | Default 10, at most 100. |


<a name="rill-v1-FindSimilarIntentsResponse"></a>

### FindSimilarIntentsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `results` | [ScoredIntent](#rill-v1-ScoredIntent) | repeated |  |


<a name="rill-v1-GetIntentRequest"></a>

### GetIntentRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/intents/{intent}`. Required. |


<a name="rill-v1-GetIntentResponse"></a>

### GetIntentResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent` | [Intent](#rill-v1-Intent) |  |  |


<a name="rill-v1-LinkChangeRequest"></a>

### LinkChangeRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/intents/{intent}`. Required. |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |


<a name="rill-v1-LinkChangeResponse"></a>

### LinkChangeResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent` | [Intent](#rill-v1-Intent) |  |  |


<a name="rill-v1-ListIntentsRequest"></a>

### ListIntentsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `filter` | [string](#string) |  | On `status` (`open`, `in_progress`, `done`, `abandoned`), `kind` (`feature`, `bug`, `refactor`, `chore`, `experiment`, `system`), `owner` (`ann`) and `parent` (an intent id). |


<a name="rill-v1-ListIntentsResponse"></a>

### ListIntentsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intents` | [Intent](#rill-v1-Intent) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-RevokeIntentApprovalRequest"></a>

### RevokeIntentApprovalRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/intents/{intent}`. Required. |


<a name="rill-v1-RevokeIntentApprovalResponse"></a>

### RevokeIntentApprovalResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent` | [Intent](#rill-v1-Intent) |  |  |


<a name="rill-v1-ScoredIntent"></a>

### ScoredIntent



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent` | [Intent](#rill-v1-Intent) |  |  |
| `score` | [float](#float) |  |  |


<a name="rill-v1-UpdateIntentRequest"></a>

### UpdateIntentRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `intent` | [Intent](#rill-v1-Intent) |  | The intent, by `name`, with the fields to change. Required. |
| `update_mask` | [google.protobuf.FieldMask](#google-protobuf-FieldMask) |  | Which of `title`, `body`, `status`, `parent`, `risk_override` and `scope` to change; a masked field left empty is cleared. Required. |
| `etag` | [string](#string) |  | The etag read; empty means no condition. |


<a name="rill-v1-UpdateIntentResponse"></a>

### UpdateIntentResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `intent` | [Intent](#rill-v1-Intent) |  |  |




## rill/v1/landing.proto

Landing changes: asking for a change or a stack to land, and following
the request through the queue.

A land request is `orgs/{org}/repos/{repo}/landRequests/{ticket}`.

<a name="rill-v1-LandingService"></a>

### LandingService

Land requests and the landing queue.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `RequestLand` | [RequestLandRequest](#rill-v1-RequestLandRequest) | [RequestLandResponse](#rill-v1-RequestLandResponse) | Ask for a change to land on trunk or a release. |
| `RequestLandStack` | [RequestLandStackRequest](#rill-v1-RequestLandStackRequest) | [RequestLandStackResponse](#rill-v1-RequestLandStackResponse) | Ask for a change's stack, from its lowest unlanded change up, to land as one ordered group. |
| `GetLandRequest` | [GetLandRequestRequest](#rill-v1-GetLandRequestRequest) | [GetLandRequestResponse](#rill-v1-GetLandRequestResponse) | A land request, with its place in the queue or its outcome. |
| `ListLandRequests` | [ListLandRequestsRequest](#rill-v1-ListLandRequestsRequest) | [ListLandRequestsResponse](#rill-v1-ListLandRequestsResponse) | The requests still waiting, in serve order. |
| `CancelLandRequest` | [CancelLandRequestRequest](#rill-v1-CancelLandRequestRequest) | [CancelLandRequestResponse](#rill-v1-CancelLandRequestResponse) | Take a request that has not started landing out of the queue. |



<a name="rill-v1-CancelLandRequestRequest"></a>

### CancelLandRequestRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/landRequests/{ticket}`. Required. |
| `reason` | [string](#string) |  | Why, for the record. |


<a name="rill-v1-CancelLandRequestResponse"></a>

### CancelLandRequestResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `land_request` | [LandRequest](#rill-v1-LandRequest) |  |  |


<a name="rill-v1-GetLandRequestRequest"></a>

### GetLandRequestRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/landRequests/{ticket}`. Required. |


<a name="rill-v1-GetLandRequestResponse"></a>

### GetLandRequestResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `land_request` | [LandRequest](#rill-v1-LandRequest) |  |  |


<a name="rill-v1-LandRequest"></a>

### LandRequest

A request for a change to land.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/landRequests/{ticket}`. |
| `ticket` | [string](#string) |  | The ticket id. |
| `change` | [string](#string) |  | `…/changes/{change}`. |
| `change_id` | [bytes](#bytes) |  |  |
| `status` | [LandStatus](#rill-v1-LandStatus) |  |  |
| `requested_at` | [int64](#int64) |  |  |
| `trunk_position` | [uint64](#uint64) | optional | Set once landed. |
| `landed_hash` | [bytes](#bytes) | optional |  |
| `error` | [string](#string) | optional | Why it did not land. |
| `queue_position` | [uint32](#uint32) | optional | While queued: its place in the serve order (0 = next) and the wait estimated from the recent landing rate. |
| `eta_ms` | [uint64](#uint64) | optional |  |


<a name="rill-v1-ListLandRequestsRequest"></a>

### ListLandRequestsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListLandRequestsResponse"></a>

### ListLandRequestsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `land_requests` | [LandRequest](#rill-v1-LandRequest) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-RequestLandRequest"></a>

### RequestLandRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |
| `onto` | [string](#string) |  | The line to land on: `trunk` (the default) or `release/{name}`. |


<a name="rill-v1-RequestLandResponse"></a>

### RequestLandResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `land_request` | [LandRequest](#rill-v1-LandRequest) |  |  |


<a name="rill-v1-RequestLandStackRequest"></a>

### RequestLandStackRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `change` | [string](#string) |  | The top of the stack: `…/changes/{change}`. Required. |
| `prefix` | [bool](#bool) |  | Land the longest landable run from the bottom instead of all or none. |
| `onto` | [string](#string) |  | The line to land on: `trunk` (the default) or `release/{name}`. |


<a name="rill-v1-RequestLandStackResponse"></a>

### RequestLandStackResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `land_requests` | [LandRequest](#rill-v1-LandRequest) | repeated | Bottom first. |



<a name="rill-v1-LandStatus"></a>

### LandStatus



| Name | Number | Description |
| --- | --- | --- |
| `LAND_STATUS_UNSPECIFIED` | 0 |  |
| `LAND_STATUS_QUEUED` | 1 |  |
| `LAND_STATUS_REBASING` | 2 |  |
| `LAND_STATUS_LANDED` | 3 |  |
| `LAND_STATUS_FAILED` | 4 |  |
| `LAND_STATUS_CANCELLED` | 5 |  |



## rill/v1/notify.proto

Notifications: a repository's webhooks (every event of chosen kinds,
signed per Standard Webhooks), and each person's channels (Slack,
Teams, email) for what concerns them, plus daily digests.

A webhook is `orgs/{org}/repos/{repo}/webhooks/{webhook}`.

<a name="rill-v1-NotificationService"></a>

### NotificationService

Webhooks and notification preferences.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `ListWebhooks` | [ListWebhooksRequest](#rill-v1-ListWebhooksRequest) | [ListWebhooksResponse](#rill-v1-ListWebhooksResponse) | The repository's webhooks (repository owners). |
| `CreateWebhook` | [CreateWebhookRequest](#rill-v1-CreateWebhookRequest) | [CreateWebhookResponse](#rill-v1-CreateWebhookResponse) | Add a webhook (repository owners). |
| `DeleteWebhook` | [DeleteWebhookRequest](#rill-v1-DeleteWebhookRequest) | [DeleteWebhookResponse](#rill-v1-DeleteWebhookResponse) | Remove a webhook (repository owners). |
| `GetNotificationPrefs` | [GetNotificationPrefsRequest](#rill-v1-GetNotificationPrefsRequest) | [GetNotificationPrefsResponse](#rill-v1-GetNotificationPrefsResponse) | The caller's preferences in a repository. |
| `UpdateNotificationPrefs` | [UpdateNotificationPrefsRequest](#rill-v1-UpdateNotificationPrefsRequest) | [UpdateNotificationPrefsResponse](#rill-v1-UpdateNotificationPrefsResponse) | Replace the caller's preferences in a repository. |
| `SendTestNotification` | [SendTestNotificationRequest](#rill-v1-SendTestNotificationRequest) | [SendTestNotificationResponse](#rill-v1-SendTestNotificationResponse) | Send a test message to each of the caller's channels. |



<a name="rill-v1-CreateWebhookRequest"></a>

### CreateWebhookRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `webhook` | [Webhook](#rill-v1-Webhook) |  |  |


<a name="rill-v1-CreateWebhookResponse"></a>

### CreateWebhookResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `webhook` | [Webhook](#rill-v1-Webhook) |  |  |


<a name="rill-v1-DeleteWebhookRequest"></a>

### DeleteWebhookRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/webhooks/{webhook}`. Required. |


<a name="rill-v1-DeleteWebhookResponse"></a>

### DeleteWebhookResponse




<a name="rill-v1-GetNotificationPrefsRequest"></a>

### GetNotificationPrefsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |


<a name="rill-v1-GetNotificationPrefsResponse"></a>

### GetNotificationPrefsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `prefs` | [NotificationPrefs](#rill-v1-NotificationPrefs) |  |  |


<a name="rill-v1-ListWebhooksRequest"></a>

### ListWebhooksRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |


<a name="rill-v1-ListWebhooksResponse"></a>

### ListWebhooksResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `webhooks` | [Webhook](#rill-v1-Webhook) | repeated |  |


<a name="rill-v1-NotificationChannel"></a>

### NotificationChannel



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `slack` | [string](#string) |  | A Slack incoming-webhook URL. |
| `teams` | [string](#string) |  | A Microsoft Teams incoming-webhook URL. |
| `email` | [string](#string) |  | An email address. |


<a name="rill-v1-NotificationPrefs"></a>

### NotificationPrefs



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `channels` | [NotificationChannel](#rill-v1-NotificationChannel) | repeated |  |
| `topics` | [string](#string) | repeated | `addressed`, `own_changes`, `audits`; empty is all. |
| `digest` | [bool](#bool) |  | A daily digest of the intents the caller owns. |


<a name="rill-v1-SendTestNotificationRequest"></a>

### SendTestNotificationRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |


<a name="rill-v1-SendTestNotificationResponse"></a>

### SendTestNotificationResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `queued` | [uint32](#uint32) |  | Deliveries queued. |


<a name="rill-v1-UpdateNotificationPrefsRequest"></a>

### UpdateNotificationPrefsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `prefs` | [NotificationPrefs](#rill-v1-NotificationPrefs) |  |  |


<a name="rill-v1-UpdateNotificationPrefsResponse"></a>

### UpdateNotificationPrefsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `prefs` | [NotificationPrefs](#rill-v1-NotificationPrefs) |  |  |


<a name="rill-v1-Webhook"></a>

### Webhook



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/webhooks/{webhook}`. Output only. |
| `url` | [string](#string) |  | `https://` (or `http://` where the server allows it). |
| `secret` | [string](#string) |  | The org secret whose value signs deliveries; empty for unsigned. |
| `events` | [string](#string) | repeated | Event kinds (`landed`, `review_submitted`, …); empty is every event. |
| `created_by` | [Principal](#rill-v1-Principal) |  | Output only. |
| `created_at` | [int64](#int64) |  | Output only. |




## rill/v1/operations.proto

Long-running operations: work that outlives the request that started
it, polled or waited on by name.

An operation is `orgs/{org}/repos/{repo}/operations/{operation}`.

<a name="rill-v1-OperationService"></a>

### OperationService

Long-running operations.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `GetOperation` | [GetOperationRequest](#rill-v1-GetOperationRequest) | [GetOperationResponse](#rill-v1-GetOperationResponse) | An operation's state, and its result once done. |
| `ListOperations` | [ListOperationsRequest](#rill-v1-ListOperationsRequest) | [ListOperationsResponse](#rill-v1-ListOperationsResponse) | A repository's operations, newest first. |
| `WaitOperation` | [WaitOperationRequest](#rill-v1-WaitOperationRequest) | [WaitOperationResponse](#rill-v1-WaitOperationResponse) | Wait until an operation is done or the timeout passes. |



<a name="rill-v1-BulkRevertEntry"></a>

### BulkRevertEntry

One landing a bulk revert covers.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landing` | [string](#string) |  | `…/trunk/positions/{n}`. |
| `position` | [uint64](#uint64) |  |  |
| `change_id` | [bytes](#bytes) |  |  |
| `summary` | [string](#string) |  |  |
| `conflicts` | [string](#string) | repeated | In a preview: paths its revert conflicts at after the newer reverts. |
| `land_request` | [string](#string) |  | Once applied: the revert's land request, `…/landRequests/{ticket}`. |


<a name="rill-v1-BulkRevertResult"></a>

### BulkRevertResult

What a bulk revert queued.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `entries` | [BulkRevertEntry](#rill-v1-BulkRevertEntry) | repeated | Newest first. |


<a name="rill-v1-GetOperationRequest"></a>

### GetOperationRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/operations/{operation}`. Required. |


<a name="rill-v1-GetOperationResponse"></a>

### GetOperationResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `operation` | [Operation](#rill-v1-Operation) |  |  |


<a name="rill-v1-ListOperationsRequest"></a>

### ListOperationsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `filter` | [string](#string) |  | On `kind` and `done`. |


<a name="rill-v1-ListOperationsResponse"></a>

### ListOperationsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `operations` | [Operation](#rill-v1-Operation) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-Operation"></a>

### Operation

Work that outlives its request.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/operations/{operation}`. Output only. |
| `kind` | [string](#string) |  | What the operation does (`bulk_revert`). |
| `done` | [bool](#bool) |  |  |
| `creator` | [Principal](#rill-v1-Principal) |  |  |
| `created_at` | [int64](#int64) |  |  |
| `updated_at` | [int64](#int64) |  |  |
| `error` | [google.rpc.Status](#google-rpc-Status) |  |  |
| `bulk_revert` | [BulkRevertResult](#rill-v1-BulkRevertResult) |  |  |


<a name="rill-v1-WaitOperationRequest"></a>

### WaitOperationRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/operations/{operation}`. Required. |
| `timeout_ms` | [uint32](#uint32) |  | At most this long; default and maximum 60 s. |


<a name="rill-v1-WaitOperationResponse"></a>

### WaitOperationResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `operation` | [Operation](#rill-v1-Operation) |  | Done, or as it stood when the timeout passed. |




## rill/v1/orgs.proto

Organizations: the tenants that own repositories, users and keys.

An organization is `orgs/{org}`.

<a name="rill-v1-OrgService"></a>

### OrgService

Organizations.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `CreateOrg` | [CreateOrgRequest](#rill-v1-CreateOrgRequest) | [CreateOrgResponse](#rill-v1-CreateOrgResponse) | Create an organization (instance administrators). |
| `GetOrg` | [GetOrgRequest](#rill-v1-GetOrgRequest) | [GetOrgResponse](#rill-v1-GetOrgResponse) | One organization. |
| `ListOrgs` | [ListOrgsRequest](#rill-v1-ListOrgsRequest) | [ListOrgsResponse](#rill-v1-ListOrgsResponse) | Every organization. |



<a name="rill-v1-CreateOrgRequest"></a>

### CreateOrgRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `org_id` | [string](#string) |  | 1-63 lowercase letters, digits and dashes. Required. |
| `description` | [string](#string) | optional |  |


<a name="rill-v1-CreateOrgResponse"></a>

### CreateOrgResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `org` | [Org](#rill-v1-Org) |  |  |


<a name="rill-v1-GetOrgRequest"></a>

### GetOrgRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}`. Required. |


<a name="rill-v1-GetOrgResponse"></a>

### GetOrgResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `org` | [Org](#rill-v1-Org) |  |  |


<a name="rill-v1-ListOrgsRequest"></a>

### ListOrgsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListOrgsResponse"></a>

### ListOrgsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `orgs` | [Org](#rill-v1-Org) | repeated |  |
| `next_page_token` | [string](#string) |  |  |




## rill/v1/policy.proto

Policies: a repository's submit and risk rules, as Cedar text.

A policy is `orgs/{org}/repos/{repo}/policies/{policy}` (`submit` or
`risk`).

<a name="rill-v1-PolicyService"></a>

### PolicyService

Repository policies.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `GetPolicy` | [GetPolicyRequest](#rill-v1-GetPolicyRequest) | [GetPolicyResponse](#rill-v1-GetPolicyResponse) | A policy; the default text when it was never set. |
| `UpdatePolicy` | [UpdatePolicyRequest](#rill-v1-UpdatePolicyRequest) | [UpdatePolicyResponse](#rill-v1-UpdatePolicyResponse) | Replace a policy's text (repository owners). |
| `GetEffectivePolicy` | [GetEffectivePolicyRequest](#rill-v1-GetEffectivePolicyRequest) | [GetEffectivePolicyResponse](#rill-v1-GetEffectivePolicyResponse) | The submit rules in force: trunk's `.rill/policy/*.cedar`, else the stored policy, else the default, with the org's guardrails after them. |
| `ExplainLanding` | [ExplainLandingRequest](#rill-v1-ExplainLandingRequest) | [ExplainLandingResponse](#rill-v1-ExplainLandingResponse) | Why a landing was allowed: the policy and the decision recorded. |
| `SimulatePolicy` | [SimulatePolicyRequest](#rill-v1-SimulatePolicyRequest) | [SimulatePolicyResponse](#rill-v1-SimulatePolicyResponse) | Replay recorded landings under a candidate policy. |
| `GetGuardrails` | [GetGuardrailsRequest](#rill-v1-GetGuardrailsRequest) | [GetGuardrailsResponse](#rill-v1-GetGuardrailsResponse) | An org's guardrails: forbid rules above every repository's policy. |
| `UpdateGuardrails` | [UpdateGuardrailsRequest](#rill-v1-UpdateGuardrailsRequest) | [UpdateGuardrailsResponse](#rill-v1-UpdateGuardrailsResponse) | Replace an org's guardrails (org administrators). |



<a name="rill-v1-ExplainLandingRequest"></a>

### ExplainLandingRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landing` | [string](#string) |  | `…/trunk/positions/{n}`. Required. |


<a name="rill-v1-ExplainLandingResponse"></a>

### ExplainLandingResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [string](#string) |  | `…/changes/{change}`. |
| `policy` | [string](#string) |  | The policy digest the landing recorded, and where the policy came from. |
| `source` | [string](#string) |  |  |
| `allowed` | [bool](#bool) |  |  |
| `matched_policies` | [string](#string) | repeated |  |
| `reasons` | [string](#string) | repeated |  |
| `facts_json` | [string](#string) |  | The change's facts and review facts the decision saw, as JSON. |
| `policy_text` | [string](#string) |  | The policy text, when this repository kept it. |


<a name="rill-v1-GetEffectivePolicyRequest"></a>

### GetEffectivePolicyRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |


<a name="rill-v1-GetEffectivePolicyResponse"></a>

### GetEffectivePolicyResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `text` | [string](#string) |  |  |
| `source` | [string](#string) |  | `default`, `stored`, or the `.rill/policy/` files, with ` + org guardrails` when the org has any. |
| `digest` | [string](#string) |  | BLAKE3 of `text`, hex: what landings record. |


<a name="rill-v1-GetGuardrailsRequest"></a>

### GetGuardrailsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `org` | [string](#string) |  | `orgs/{org}`. Required. |


<a name="rill-v1-GetGuardrailsResponse"></a>

### GetGuardrailsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `text` | [string](#string) |  |  |


<a name="rill-v1-GetPolicyRequest"></a>

### GetPolicyRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/policies/{policy}`. Required. |


<a name="rill-v1-GetPolicyResponse"></a>

### GetPolicyResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `policy` | [Policy](#rill-v1-Policy) |  |  |


<a name="rill-v1-Policy"></a>

### Policy

A repository policy.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/policies/{policy}`. |
| `text` | [string](#string) |  | Cedar policy text. |
| `updated_at` | [int64](#int64) |  | Output only. |
| `updated_by` | [Principal](#rill-v1-Principal) |  | Output only. |
| `etag` | [string](#string) |  | Output only. |


<a name="rill-v1-SimulatePolicyRequest"></a>

### SimulatePolicyRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `text` | [string](#string) |  | The candidate Cedar text. Required. |
| `since` | [uint64](#uint64) |  | Trunk positions to replay, inclusive; 0 for the first or the last. |
| `until` | [uint64](#uint64) |  |  |


<a name="rill-v1-SimulatePolicyResponse"></a>

### SimulatePolicyResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `replayed` | [uint32](#uint32) |  | Landings replayed (those with a recorded decision). |
| `unrecorded` | [uint32](#uint32) |  | Landings in range without a recorded decision. |
| `allowed` | [uint32](#uint32) |  | Still allowed under the candidate. |
| `blocked` | [SimulatedLanding](#rill-v1-SimulatedLanding) | repeated | Allowed then, blocked under the candidate. |
| `agent_only_before` | [uint32](#uint32) |  | Landings that could have landed with no human vote: under the policy recorded, and under the candidate. |
| `agent_only_after` | [uint32](#uint32) |  |  |


<a name="rill-v1-SimulatedLanding"></a>

### SimulatedLanding



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landing` | [string](#string) |  | `…/trunk/positions/{n}`. |
| `change` | [string](#string) |  |  |
| `reasons` | [string](#string) | repeated |  |


<a name="rill-v1-UpdateGuardrailsRequest"></a>

### UpdateGuardrailsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `org` | [string](#string) |  | `orgs/{org}`. Required. |
| `text` | [string](#string) |  | Cedar `forbid` rules, each with an `@id` starting `org_`; empty removes them. |


<a name="rill-v1-UpdateGuardrailsResponse"></a>

### UpdateGuardrailsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `text` | [string](#string) |  |  |


<a name="rill-v1-UpdatePolicyRequest"></a>

### UpdatePolicyRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `policy` | [Policy](#rill-v1-Policy) |  | The policy, by `name`, with its new text. Required. |
| `etag` | [string](#string) |  | The etag read; empty means no condition. |


<a name="rill-v1-UpdatePolicyResponse"></a>

### UpdatePolicyResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `policy` | [Policy](#rill-v1-Policy) |  |  |




## rill/v1/protection.proto

Protection: redacting objects (two administrators), releasing versions
secret scanning quarantined, and rotating data keys.

A redaction is `orgs/{org}/redactions/{redaction}`.

<a name="rill-v1-ProtectionService"></a>

### ProtectionService

Redactions, quarantined versions and data keys.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `CreateRedaction` | [CreateRedactionRequest](#rill-v1-CreateRedactionRequest) | [CreateRedactionResponse](#rill-v1-CreateRedactionResponse) | Ask for an object to be redacted; another administrator approves. |
| `ApproveRedaction` | [ApproveRedactionRequest](#rill-v1-ApproveRedactionRequest) | [ApproveRedactionResponse](#rill-v1-ApproveRedactionResponse) | Apply a request; the approver must be another administrator. |
| `ListRedactions` | [ListRedactionsRequest](#rill-v1-ListRedactionsRequest) | [ListRedactionsResponse](#rill-v1-ListRedactionsResponse) | An organization's redactions, pending and applied. |
| `ReleaseVersion` | [ReleaseVersionRequest](#rill-v1-ReleaseVersionRequest) | [ReleaseVersionResponse](#rill-v1-ReleaseVersionResponse) | Release a version secret scanning quarantined, so it may land. |
| `RotateDataKeys` | [RotateDataKeysRequest](#rill-v1-RotateDataKeysRequest) | [RotateDataKeysResponse](#rill-v1-RotateDataKeysResponse) | Add a data key to an org (it seals after the activation delay), or wrap every data key again under the key service's current key. |



<a name="rill-v1-ApproveRedactionRequest"></a>

### ApproveRedactionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `orgs/{org}/redactions/{redaction}`. Required. |


<a name="rill-v1-ApproveRedactionResponse"></a>

### ApproveRedactionResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `redaction` | [Redaction](#rill-v1-Redaction) |  |  |


<a name="rill-v1-CreateRedactionRequest"></a>

### CreateRedactionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `kind` | [ObjectKind](#rill-v1-ObjectKind) |  |  |
| `hash` | [bytes](#bytes) |  | Required. |
| `reason` | [string](#string) |  | Required. |


<a name="rill-v1-CreateRedactionResponse"></a>

### CreateRedactionResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `redaction` | [Redaction](#rill-v1-Redaction) |  |  |


<a name="rill-v1-ListRedactionsRequest"></a>

### ListRedactionsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListRedactionsResponse"></a>

### ListRedactionsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `redactions` | [Redaction](#rill-v1-Redaction) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-Redaction"></a>

### Redaction



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/redactions/{redaction}`. |
| `id` | [string](#string) |  |  |
| `org` | [string](#string) |  |  |
| `kind` | [ObjectKind](#rill-v1-ObjectKind) |  |  |
| `hash` | [bytes](#bytes) |  |  |
| `reason` | [string](#string) |  |  |
| `requested_by` | [Principal](#rill-v1-Principal) |  |  |
| `requested_at` | [int64](#int64) |  |  |
| `approved_by` | [Principal](#rill-v1-Principal) | optional |  |
| `applied_at` | [int64](#int64) | optional |  |


<a name="rill-v1-ReleaseVersionRequest"></a>

### ReleaseVersionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `version` | [string](#string) |  | `orgs/{org}/repos/{repo}/versions/{hash}`. Required. |


<a name="rill-v1-ReleaseVersionResponse"></a>

### ReleaseVersionResponse




<a name="rill-v1-RotateDataKeysRequest"></a>

### RotateDataKeysRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}`, whose key to rotate; ignored with rewrap. |
| `rewrap` | [bool](#bool) |  |  |


<a name="rill-v1-RotateDataKeysResponse"></a>

### RotateDataKeysResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `key_id` | [uint32](#uint32) | optional | The new key; absent when another server added one at the same moment. |
| `active_from` | [int64](#int64) |  |  |
| `rewrapped` | [uint32](#uint32) |  |  |




## rill/v1/repos.proto

Repositories, their refs and trunk.

A repository is `orgs/{org}/repos/{repo}`. A revision string names a
point in its history: `trunk` (the default), `trunk@N`, a version hash
(64 hex digits), `tree:<hex>`, `changes/{change}` (its current version),
`workspaces/{workspace}` (its snapshot) or a ref name.

<a name="rill-v1-RepoService"></a>

### RepoService

Repositories, refs, releases and trunk positions.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `GetRepo` | [GetRepoRequest](#rill-v1-GetRepoRequest) | [GetRepoResponse](#rill-v1-GetRepoResponse) | A repository by name. |
| `ListRepos` | [ListReposRequest](#rill-v1-ListReposRequest) | [ListReposResponse](#rill-v1-ListReposResponse) | The repositories of an org, by repository id. |
| `CreateRepo` | [CreateRepoRequest](#rill-v1-CreateRepoRequest) | [CreateRepoResponse](#rill-v1-CreateRepoResponse) | Create a repository. |
| `ResolveRevision` | [ResolveRevisionRequest](#rill-v1-ResolveRevisionRequest) | [ResolveRevisionResponse](#rill-v1-ResolveRevisionResponse) | What a revision string names: its version, tree and trunk position. |
| `GetRef` | [GetRefRequest](#rill-v1-GetRefRequest) | [GetRefResponse](#rill-v1-GetRefResponse) | A ref by name. |
| `ListRefs` | [ListRefsRequest](#rill-v1-ListRefsRequest) | [ListRefsResponse](#rill-v1-ListRefsResponse) | The refs of a repository, by name. |
| `UpdateRef` | [UpdateRefRequest](#rill-v1-UpdateRefRequest) | [UpdateRefResponse](#rill-v1-UpdateRefResponse) | Point a ref at a version, if it still has the `etag` given. |
| `DeleteRef` | [DeleteRefRequest](#rill-v1-DeleteRefRequest) | [DeleteRefResponse](#rill-v1-DeleteRefResponse) | Delete a ref, if it still has the `etag` given. |
| `CreateRelease` | [CreateReleaseRequest](#rill-v1-CreateReleaseRequest) | [CreateReleaseResponse](#rill-v1-CreateReleaseResponse) | Cut `release/{release_id}` at a trunk position. A release then moves only through its own lander (LandingService.RequestLand with `onto`). |
| `GetTrunkPosition` | [GetTrunkPositionRequest](#rill-v1-GetTrunkPositionRequest) | [GetTrunkPositionResponse](#rill-v1-GetTrunkPositionResponse) | A trunk position: `…/trunk/positions/{N}` or `…/trunk/positions/head`. |
| `ListTrunkPositions` | [ListTrunkPositionsRequest](#rill-v1-ListTrunkPositionsRequest) | [ListTrunkPositionsResponse](#rill-v1-ListTrunkPositionsResponse) | Trunk positions in order. |
| `WatchTrunk` | [WatchTrunkRequest](#rill-v1-WatchTrunkRequest) | [WatchTrunkResponse](#rill-v1-WatchTrunkResponse) stream | Landings as they happen, resuming after a position. |
| `WatchRefs` | [WatchRefsRequest](#rill-v1-WatchRefsRequest) | [WatchRefsResponse](#rill-v1-WatchRefsResponse) stream | Ref moves as they happen. |
| `WatchVersions` | [WatchVersionsRequest](#rill-v1-WatchVersionsRequest) | [WatchVersionsResponse](#rill-v1-WatchVersionsResponse) stream | Every version stored, resumable by cursor. |
| `GetDerivedValue` | [GetDerivedValueRequest](#rill-v1-GetDerivedValueRequest) | [GetDerivedValueResponse](#rill-v1-GetDerivedValueResponse) | A derived value of a version (an index, a summary), if computed. |
| `WaitDerivedValue` | [WaitDerivedValueRequest](#rill-v1-WaitDerivedValueRequest) | [WaitDerivedValueResponse](#rill-v1-WaitDerivedValueResponse) | Wait for a derived value to be computed. |



<a name="rill-v1-CreateReleaseRequest"></a>

### CreateReleaseRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `release_id` | [string](#string) |  | The release's name: the ref is `release/{release_id}`. Required. |
| `trunk_position` | [uint64](#uint64) | optional | The trunk position to cut at; the head when absent. |


<a name="rill-v1-CreateReleaseResponse"></a>

### CreateReleaseResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `ref` | [Ref](#rill-v1-Ref) |  |  |


<a name="rill-v1-CreateRepoRequest"></a>

### CreateRepoRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  | Makes the call safe to retry: a UUID. |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `repo_id` | [string](#string) |  | The repository's name within the org. Required. |
| `repo` | [Repo](#rill-v1-Repo) |  | `description` and `windows_safe` are read; the rest is output only. |


<a name="rill-v1-CreateRepoResponse"></a>

### CreateRepoResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [Repo](#rill-v1-Repo) |  |  |


<a name="rill-v1-DeleteRefRequest"></a>

### DeleteRefRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/refs/{ref}`. Required. |
| `etag` | [string](#string) |  | The ref's current etag. Required. |


<a name="rill-v1-DeleteRefResponse"></a>

### DeleteRefResponse




<a name="rill-v1-GetDerivedValueRequest"></a>

### GetDerivedValueRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `kind` | [string](#string) |  |  |
| `hash` | [bytes](#bytes) |  |  |


<a name="rill-v1-GetDerivedValueResponse"></a>

### GetDerivedValueResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `data` | [bytes](#bytes) |  |  |


<a name="rill-v1-GetRefRequest"></a>

### GetRefRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/refs/{ref}`; `{ref}` may hold slashes. Required. |


<a name="rill-v1-GetRefResponse"></a>

### GetRefResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `ref` | [Ref](#rill-v1-Ref) |  |  |


<a name="rill-v1-GetRepoRequest"></a>

### GetRepoRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}`; `{repo}` may also be the `rp_` id. Required. |


<a name="rill-v1-GetRepoResponse"></a>

### GetRepoResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [Repo](#rill-v1-Repo) |  |  |


<a name="rill-v1-GetTrunkPositionRequest"></a>

### GetTrunkPositionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/repos/{repo}/trunk/positions/{N}`, or `…/head`. Required. |


<a name="rill-v1-GetTrunkPositionResponse"></a>

### GetTrunkPositionResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landing` | [Landing](#rill-v1-Landing) |  |  |


<a name="rill-v1-ListRefsRequest"></a>

### ListRefsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `filter` | [string](#string) |  | On `name` (the short name, `release/1.0`), e.g. `name = "release/*"`. |


<a name="rill-v1-ListRefsResponse"></a>

### ListRefsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `refs` | [Ref](#rill-v1-Ref) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListReposRequest"></a>

### ListReposRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListReposResponse"></a>

### ListReposResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repos` | [Repo](#rill-v1-Repo) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListTrunkPositionsRequest"></a>

### ListTrunkPositionsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `filter` | [string](#string) |  | On `position`, e.g. `position >= 100 AND position <= 200`. |


<a name="rill-v1-ListTrunkPositionsResponse"></a>

### ListTrunkPositionsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landings` | [Landing](#rill-v1-Landing) | repeated | Oldest first. |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ResolveRevisionRequest"></a>

### ResolveRevisionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `revision` | [string](#string) |  | Defaults to `trunk`. |


<a name="rill-v1-ResolveRevisionResponse"></a>

### ResolveRevisionResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `version` | [string](#string) |  | `orgs/{org}/repos/{repo}/versions/{hash}`; empty for a bare tree. |
| `version_hash` | [bytes](#bytes) |  |  |
| `tree_hash` | [bytes](#bytes) |  |  |
| `trunk_position` | [uint64](#uint64) | optional | Set when the version is on trunk. |


<a name="rill-v1-UpdateRefRequest"></a>

### UpdateRefRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `ref` | [Ref](#rill-v1-Ref) |  | `name` and `target` are read. Required. |
| `etag` | [string](#string) |  | The ref's current etag; the update fails with ETAG_MISMATCH otherwise. Empty with `allow_missing` creates the ref, failing if it exists. |
| `allow_missing` | [bool](#bool) |  | Create the ref if it does not exist. |


<a name="rill-v1-UpdateRefResponse"></a>

### UpdateRefResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `ref` | [Ref](#rill-v1-Ref) |  |  |


<a name="rill-v1-WaitDerivedValueRequest"></a>

### WaitDerivedValueRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `kind` | [string](#string) |  |  |
| `hash` | [bytes](#bytes) |  |  |
| `timeout_ms` | [uint64](#uint64) |  |  |


<a name="rill-v1-WaitDerivedValueResponse"></a>

### WaitDerivedValueResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `ready` | [bool](#bool) |  |  |
| `data` | [bytes](#bytes) | optional |  |


<a name="rill-v1-WatchRefsRequest"></a>

### WatchRefsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `prefix` | [string](#string) |  | Only refs whose short name starts with this. |


<a name="rill-v1-WatchRefsResponse"></a>

### WatchRefsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `ref` | [string](#string) |  | The ref's short name. |
| `old` | [bytes](#bytes) | optional |  |
| `new` | [bytes](#bytes) | optional |  |
| `at` | [int64](#int64) |  |  |


<a name="rill-v1-WatchTrunkRequest"></a>

### WatchTrunkRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `after_position` | [uint64](#uint64) | optional | Resume after this position; absent sends only new landings. |


<a name="rill-v1-WatchTrunkResponse"></a>

### WatchTrunkResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landing` | [Landing](#rill-v1-Landing) |  |  |


<a name="rill-v1-WatchVersionsRequest"></a>

### WatchVersionsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `after` | [bytes](#bytes) | optional | Absent: only versions stored from now on. Empty: the whole log. Otherwise resume after this cursor, from an earlier response. |


<a name="rill-v1-WatchVersionsResponse"></a>

### WatchVersionsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `cursor` | [bytes](#bytes) |  | Pass as `after` to resume after this version. |
| `version` | [ChangeInfo](#rill-v1-ChangeInfo) |  |  |




## rill/v1/review.proto

Reviews: votes, threads, stacks and whether a change may land.

A change's review is `orgs/{org}/repos/{repo}/changes/{change}/review`;
a thread on it is `…/review/threads/{thread}`.

<a name="rill-v1-ReviewService"></a>

### ReviewService

Reviews of changes.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `GetReview` | [GetReviewRequest](#rill-v1-GetReviewRequest) | [GetReviewResponse](#rill-v1-GetReviewResponse) | A change's review and the labels it is voted on. |
| `BatchGetReviews` | [BatchGetReviewsRequest](#rill-v1-BatchGetReviewsRequest) | [BatchGetReviewsResponse](#rill-v1-BatchGetReviewsResponse) | Up to 1,000 reviews. |
| `ListReviews` | [ListReviewsRequest](#rill-v1-ListReviewsRequest) | [ListReviewsResponse](#rill-v1-ListReviewsResponse) | A repository's reviews. |
| `SubmitChange` | [SubmitChangeRequest](#rill-v1-SubmitChangeRequest) | [SubmitChangeResponse](#rill-v1-SubmitChangeResponse) | Submit a change for review. |
| `Vote` | [VoteRequest](#rill-v1-VoteRequest) | [VoteResponse](#rill-v1-VoteResponse) | Vote on a label. |
| `CreateThread` | [CreateThreadRequest](#rill-v1-CreateThreadRequest) | [CreateThreadResponse](#rill-v1-CreateThreadResponse) | Start a thread on a review. |
| `ReplyThread` | [ReplyThreadRequest](#rill-v1-ReplyThreadRequest) | [ReplyThreadResponse](#rill-v1-ReplyThreadResponse) | Reply on a thread. |
| `ResolveThread` | [ResolveThreadRequest](#rill-v1-ResolveThreadRequest) | [ResolveThreadResponse](#rill-v1-ResolveThreadResponse) | Resolve or reopen a thread. |
| `UpdateStack` | [UpdateStackRequest](#rill-v1-UpdateStackRequest) | [UpdateStackResponse](#rill-v1-UpdateStackResponse) | Order changes into a stack, bottom first. |
| `EvaluateLanding` | [EvaluateLandingRequest](#rill-v1-EvaluateLandingRequest) | [EvaluateLandingResponse](#rill-v1-EvaluateLandingResponse) | Whether the repository's policy lets a change land now, and why not. |
| `AbandonChange` | [AbandonChangeRequest](#rill-v1-AbandonChangeRequest) | [AbandonChangeResponse](#rill-v1-AbandonChangeResponse) | Abandon a change. |
| `WatchReview` | [WatchReviewRequest](#rill-v1-WatchReviewRequest) | [WatchReviewResponse](#rill-v1-WatchReviewResponse) stream | A review's updates as they happen. |
| `GetOwners` | [GetOwnersRequest](#rill-v1-GetOwnersRequest) | [GetOwnersResponse](#rill-v1-GetOwnersResponse) | The owner sets covering a path, by trunk's `OWNERS.toml` files. |
| `ExplainOwnership` | [ExplainOwnershipRequest](#rill-v1-ExplainOwnershipRequest) | [ExplainOwnershipResponse](#rill-v1-ExplainOwnershipResponse) | Each owner set a change touches, with its approvals and escalation. |
| `GetInterdiff` | [GetInterdiffRequest](#rill-v1-GetInterdiffRequest) | [GetInterdiffResponse](#rill-v1-GetInterdiffResponse) | What the author changed between two versions of a change, with paths that only moved with a rebase listed apart. |
| `ApplySuggestion` | [ApplySuggestionRequest](#rill-v1-ApplySuggestionRequest) | [ApplySuggestionResponse](#rill-v1-ApplySuggestionResponse) | Apply a thread's suggested edit as a new version (the change's owner). |
| `ReopenChange` | [ReopenChangeRequest](#rill-v1-ReopenChangeRequest) | [ReopenChangeResponse](#rill-v1-ReopenChangeResponse) | Reopen an abandoned change. |



<a name="rill-v1-AbandonChangeRequest"></a>

### AbandonChangeRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |
| `reason` | [string](#string) | optional |  |


<a name="rill-v1-AbandonChangeResponse"></a>

### AbandonChangeResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `review` | [ReviewState](#rill-v1-ReviewState) |  |  |


<a name="rill-v1-ApplySuggestionRequest"></a>

### ApplySuggestionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `thread` | [string](#string) |  | `…/review/threads/{thread}`. Required. |


<a name="rill-v1-ApplySuggestionResponse"></a>

### ApplySuggestionResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `info` | [ChangeInfo](#rill-v1-ChangeInfo) |  |  |


<a name="rill-v1-BatchGetReviewsRequest"></a>

### BatchGetReviewsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `names` | [string](#string) | repeated | At most 1,000 review names. |


<a name="rill-v1-BatchGetReviewsResponse"></a>

### BatchGetReviewsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `reviews` | [ReviewState](#rill-v1-ReviewState) | repeated | Parallel to `names`. |


<a name="rill-v1-CreateThreadRequest"></a>

### CreateThreadRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `…/changes/{change}/review`. Required. |
| `anchor` | [ThreadAnchor](#rill-v1-ThreadAnchor) |  |  |
| `body` | [string](#string) |  | Required. |
| `category` | [CommentCategory](#rill-v1-CommentCategory) | optional |  |
| `confidence` | [uint32](#uint32) | optional |  |
| `to` | [Principal](#rill-v1-Principal) | optional |  |
| `suggestion` | [string](#string) | optional | Replacement text for the anchored lines (a `lines` anchor on the current version), which the change's author can apply. |


<a name="rill-v1-CreateThreadResponse"></a>

### CreateThreadResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `thread` | [Thread](#rill-v1-Thread) |  |  |


<a name="rill-v1-EvaluateLandingRequest"></a>

### EvaluateLandingRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |


<a name="rill-v1-EvaluateLandingResponse"></a>

### EvaluateLandingResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `decision` | [SubmitDecision](#rill-v1-SubmitDecision) |  |  |


<a name="rill-v1-ExplainOwnershipRequest"></a>

### ExplainOwnershipRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |


<a name="rill-v1-ExplainOwnershipResponse"></a>

### ExplainOwnershipResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `sets` | [OwnerSetStatus](#rill-v1-OwnerSetStatus) | repeated |  |
| `covered` | [bool](#bool) |  | Every set is covered (true when the change touches none). |


<a name="rill-v1-GetInterdiffRequest"></a>

### GetInterdiffRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |
| `from` | [string](#string) |  | The versions compared; empty `to` is the current version, empty `from` the one before it. |
| `to` | [string](#string) |  |  |


<a name="rill-v1-GetInterdiffResponse"></a>

### GetInterdiffResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `from` | [string](#string) |  |  |
| `to` | [string](#string) |  |  |
| `files` | [InterdiffFile](#rill-v1-InterdiffFile) | repeated |  |
| `rebased` | [string](#string) | repeated | Paths whose patch is the same in both: they moved with the base only. |


<a name="rill-v1-GetOwnersRequest"></a>

### GetOwnersRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `repo` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `path` | [string](#string) |  | A file path. Required. |


<a name="rill-v1-GetOwnersResponse"></a>

### GetOwnersResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `sets` | [OwnerSet](#rill-v1-OwnerSet) | repeated |  |


<a name="rill-v1-GetReviewRequest"></a>

### GetReviewRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/changes/{change}/review`. Required. |


<a name="rill-v1-GetReviewResponse"></a>

### GetReviewResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `review` | [ReviewState](#rill-v1-ReviewState) |  |  |
| `labels` | [Label](#rill-v1-Label) | repeated |  |


<a name="rill-v1-InterdiffFile"></a>

### InterdiffFile



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `path` | [string](#string) |  |  |
| `unified` | [string](#string) |  | Unified diff from the old version's content to the new one's. |
| `added` | [bool](#bool) |  |  |
| `deleted` | [bool](#bool) |  |  |


<a name="rill-v1-ListReviewsRequest"></a>

### ListReviewsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `filter` | [string](#string) |  | On `status` (`open`, `approved`, `landing`, `landed`, `abandoned`, …), `owner` (`ann`, `agent:bot`) and `intent` (an intent id). |


<a name="rill-v1-ListReviewsResponse"></a>

### ListReviewsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `reviews` | [ReviewState](#rill-v1-ReviewState) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-OwnerSet"></a>

### OwnerSet

A group of principals of which `required` must approve a change that
touches the paths it covers.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `id` | [string](#string) |  | `<dir>#default` or `<dir>#rule<n>`. |
| `dir` | [string](#string) |  | The directory whose `OWNERS.toml` declares it; empty for the root. |
| `owners` | [string](#string) | repeated | `user:<name>`, `agent:<name>` or `team:<name>`. |
| `required` | [uint32](#uint32) |  |  |
| `sensitive` | [bool](#bool) |  | Touching it makes a change at least `sensitive`. |
| `sla_ms` | [int64](#int64) | optional | How long a required review may wait before escalating. |
| `escalate_to` | [string](#string) | optional |  |


<a name="rill-v1-OwnerSetStatus"></a>

### OwnerSetStatus

One owner set's state for a change.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `set` | [OwnerSet](#rill-v1-OwnerSet) |  |  |
| `approvals` | [uint32](#uint32) |  | Distinct approvers who count for the set. |
| `covered` | [bool](#bool) |  |  |
| `escalated_to` | [string](#string) | optional | Where an overdue review was escalated to, and how many times. |
| `escalations` | [uint32](#uint32) |  |  |


<a name="rill-v1-ReopenChangeRequest"></a>

### ReopenChangeRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |


<a name="rill-v1-ReopenChangeResponse"></a>

### ReopenChangeResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `review` | [ReviewState](#rill-v1-ReviewState) |  |  |


<a name="rill-v1-ReplyThreadRequest"></a>

### ReplyThreadRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/review/threads/{thread}`. Required. |
| `body` | [string](#string) |  | Required. |
| `category` | [CommentCategory](#rill-v1-CommentCategory) | optional |  |
| `confidence` | [uint32](#uint32) | optional |  |


<a name="rill-v1-ReplyThreadResponse"></a>

### ReplyThreadResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `thread` | [Thread](#rill-v1-Thread) |  |  |


<a name="rill-v1-ResolveThreadRequest"></a>

### ResolveThreadRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/review/threads/{thread}`. Required. |
| `resolved` | [bool](#bool) |  | False reopens the thread. |


<a name="rill-v1-ResolveThreadResponse"></a>

### ResolveThreadResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `thread` | [Thread](#rill-v1-Thread) |  |  |


<a name="rill-v1-SubmitChangeRequest"></a>

### SubmitChangeRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |


<a name="rill-v1-SubmitChangeResponse"></a>

### SubmitChangeResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `review` | [ReviewState](#rill-v1-ReviewState) |  |  |


<a name="rill-v1-SubmitDecision"></a>

### SubmitDecision



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `allowed` | [bool](#bool) |  |  |
| `reasons` | [string](#string) | repeated |  |
| `matched_policies` | [string](#string) | repeated |  |
| `risk` | [RiskClass](#rill-v1-RiskClass) |  |  |
| `risk_reasons` | [string](#string) | repeated |  |


<a name="rill-v1-UpdateStackRequest"></a>

### UpdateStackRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `changes` | [string](#string) | repeated | Change names, bottom first; every change gets the same stack. |


<a name="rill-v1-UpdateStackResponse"></a>

### UpdateStackResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `reviews` | [ReviewState](#rill-v1-ReviewState) | repeated |  |


<a name="rill-v1-VoteRequest"></a>

### VoteRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `review` | [string](#string) |  | `…/changes/{change}/review`. Required. |
| `label` | [string](#string) |  | Required. |
| `value` | [int32](#int32) |  |  |
| `reason` | [string](#string) | optional |  |
| `version` | [string](#string) |  | The version voted on, `…/versions/{hash}`; the current one when empty. |


<a name="rill-v1-VoteResponse"></a>

### VoteResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `review` | [ReviewState](#rill-v1-ReviewState) |  |  |


<a name="rill-v1-WatchReviewRequest"></a>

### WatchReviewRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/changes/{change}/review`. Required. |


<a name="rill-v1-WatchReviewResponse"></a>

### WatchReviewResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change_id` | [bytes](#bytes) |  |  |
| `at` | [int64](#int64) |  |  |
| `status_changed` | [ReviewStatus](#rill-v1-ReviewStatus) |  |  |
| `voted` | [LabelVote](#rill-v1-LabelVote) |  |  |
| `thread_updated` | [Thread](#rill-v1-Thread) |  |  |
| `new_version` | [bytes](#bytes) |  |  |
| `land_update` | [LandRequest](#rill-v1-LandRequest) |  |  |




## rill/v1/reviewers.proto

The reviewer fleet: agents that review changes and resolve conflicts,
run outside the server by `rill-review-runner` or any client.

Changes are routed to reviewers by the repository's
`.rill/review.toml`; each reviewer has its own queue. A runner leases
the next job for the agent it signs in as, reviews the packet, and
reports structured findings and a vote, which the server checks before
posting them as that agent.

<a name="rill-v1-ReviewerService"></a>

### ReviewerService

Leasing and finishing review and resolve jobs.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `LeaseReviewJob` | [LeaseReviewJobRequest](#rill-v1-LeaseReviewJobRequest) | [LeaseReviewJobResponse](#rill-v1-LeaseReviewJobResponse) | Lease the calling reviewer's next review job in a repository. Waits up to `wait_ms` for one; the response has no lease when none came. |
| `RenewReviewLease` | [RenewReviewLeaseRequest](#rill-v1-RenewReviewLeaseRequest) | [RenewReviewLeaseResponse](#rill-v1-RenewReviewLeaseResponse) | Extend a review or resolve lease. |
| `CompleteReviewJob` | [CompleteReviewJobRequest](#rill-v1-CompleteReviewJobRequest) | [CompleteReviewJobResponse](#rill-v1-CompleteReviewJobResponse) | Report a review: findings become threads and the vote is cast, after checking the reviewer's maximum vote, the findings' paths and that the reviewed version is still current. |
| `FailReviewJob` | [FailReviewJobRequest](#rill-v1-FailReviewJobRequest) | [FailReviewJobResponse](#rill-v1-FailReviewJobResponse) | Give a review or resolve job up, to retry later or for good. |
| `RequestReview` | [RequestReviewRequest](#rill-v1-RequestReviewRequest) | [RequestReviewResponse](#rill-v1-RequestReviewResponse) | Queue reviews of a change's current version now. |
| `ListReviewRuns` | [ListReviewRunsRequest](#rill-v1-ListReviewRunsRequest) | [ListReviewRunsResponse](#rill-v1-ListReviewRunsResponse) | A change's review runs, newest first. |
| `LeaseResolveJob` | [LeaseResolveJobRequest](#rill-v1-LeaseResolveJobRequest) | [LeaseResolveJobResponse](#rill-v1-LeaseResolveJobResponse) | Lease the calling resolver's next conflict to resolve. |
| `CompleteResolveJob` | [CompleteResolveJobRequest](#rill-v1-CompleteResolveJobRequest) | [CompleteResolveJobResponse](#rill-v1-CompleteResolveJobResponse) | Report a resolve job: the version the resolver stored, or that it declined. |
| `ListReviewerStats` | [ListReviewerStatsRequest](#rill-v1-ListReviewerStatsRequest) | [ListReviewerStatsResponse](#rill-v1-ListReviewerStatsResponse) | Reviewers' track records in a repository and the trust they earn. |
| `ReinstateReviewer` | [ReinstateReviewerRequest](#rill-v1-ReinstateReviewerRequest) | [ReinstateReviewerResponse](#rill-v1-ReinstateReviewerResponse) | Clear a reviewer's record and lift its demotion (repository owners). |



<a name="rill-v1-CompleteResolveJobRequest"></a>

### CompleteResolveJobRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `lease` | [bytes](#bytes) |  |  |
| `resolved_version` | [string](#string) |  | The version the resolver stored with `ResolveConflicts`; empty when it declined. |
| `rationale` | [string](#string) |  |  |


<a name="rill-v1-CompleteResolveJobResponse"></a>

### CompleteResolveJobResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `run` | [ReviewRun](#rill-v1-ReviewRun) |  |  |


<a name="rill-v1-CompleteReviewJobRequest"></a>

### CompleteReviewJobRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `lease` | [bytes](#bytes) |  |  |
| `findings` | [ReviewFinding](#rill-v1-ReviewFinding) | repeated |  |
| `vote` | [int32](#int32) |  | The `Review` vote, within the reviewer's maximum; 0 casts none. |
| `summary` | [string](#string) |  |  |
| `model` | [string](#string) |  | The model that ran, as the runner reports it. |


<a name="rill-v1-CompleteReviewJobResponse"></a>

### CompleteReviewJobResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `run` | [ReviewRun](#rill-v1-ReviewRun) |  |  |


<a name="rill-v1-FailReviewJobRequest"></a>

### FailReviewJobRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `lease` | [bytes](#bytes) |  |  |
| `error` | [string](#string) |  |  |
| `retry` | [bool](#bool) |  | Try again later; otherwise the job ends now. |


<a name="rill-v1-FailReviewJobResponse"></a>

### FailReviewJobResponse




<a name="rill-v1-LeaseResolveJobRequest"></a>

### LeaseResolveJobRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `wait_ms` | [uint32](#uint32) |  |  |


<a name="rill-v1-LeaseResolveJobResponse"></a>

### LeaseResolveJobResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `lease` | [bytes](#bytes) |  |  |
| `lease_until` | [int64](#int64) |  |  |
| `job` | [ResolveJob](#rill-v1-ResolveJob) |  |  |


<a name="rill-v1-LeaseReviewJobRequest"></a>

### LeaseReviewJobRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `wait_ms` | [uint32](#uint32) |  | How long to wait for a job, in milliseconds (at most 30000). |


<a name="rill-v1-LeaseReviewJobResponse"></a>

### LeaseReviewJobResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `lease` | [bytes](#bytes) |  | Opaque; empty when no job was waiting. |
| `lease_until` | [int64](#int64) |  |  |
| `job` | [ReviewJob](#rill-v1-ReviewJob) |  |  |


<a name="rill-v1-ListReviewRunsRequest"></a>

### ListReviewRunsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |


<a name="rill-v1-ListReviewRunsResponse"></a>

### ListReviewRunsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `runs` | [ReviewRun](#rill-v1-ReviewRun) | repeated |  |


<a name="rill-v1-ListReviewerStatsRequest"></a>

### ListReviewerStatsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |


<a name="rill-v1-ListReviewerStatsResponse"></a>

### ListReviewerStatsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `reviewers` | [ReviewerStats](#rill-v1-ReviewerStats) | repeated |  |


<a name="rill-v1-ReinstateReviewerRequest"></a>

### ReinstateReviewerRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `reviewer` | [string](#string) |  | `agent:<name>` or the name. |


<a name="rill-v1-ReinstateReviewerResponse"></a>

### ReinstateReviewerResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `stats` | [ReviewerStats](#rill-v1-ReviewerStats) |  |  |


<a name="rill-v1-RenewReviewLeaseRequest"></a>

### RenewReviewLeaseRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `lease` | [bytes](#bytes) |  |  |


<a name="rill-v1-RenewReviewLeaseResponse"></a>

### RenewReviewLeaseResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `lease` | [bytes](#bytes) |  |  |
| `lease_until` | [int64](#int64) |  |  |


<a name="rill-v1-RequestReviewRequest"></a>

### RequestReviewRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `change` | [string](#string) |  | `…/changes/{change}`. Required. |
| `reviewers` | [string](#string) | repeated | Reviewer agents to ask; empty routes by `.rill/review.toml`. |


<a name="rill-v1-RequestReviewResponse"></a>

### RequestReviewResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `runs` | [ReviewRun](#rill-v1-ReviewRun) | repeated |  |


<a name="rill-v1-ResolveJob"></a>

### ResolveJob

A conflicted version to resolve.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [string](#string) |  | `…/changes/{change}`. |
| `version` | [string](#string) |  |  |
| `paths` | [string](#string) | repeated | The conflicted paths. |
| `prompt` | [string](#string) |  | The prompt file's text on trunk, when the resolver has one. |


<a name="rill-v1-ReviewFile"></a>

### ReviewFile



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `path` | [string](#string) |  |  |
| `unified` | [string](#string) |  | Unified diff against the first parent; empty for binary, oversized or dropped files. |
| `added` | [bool](#bool) |  |  |
| `deleted` | [bool](#bool) |  |  |


<a name="rill-v1-ReviewFinding"></a>

### ReviewFinding

One observation a reviewer reports.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `path` | [string](#string) |  | A touched path; empty for a change-level finding. |
| `start_line` | [uint32](#uint32) |  | One-based inclusive lines in the reviewed version; 0 for the whole file. |
| `end_line` | [uint32](#uint32) |  |  |
| `category` | [CommentCategory](#rill-v1-CommentCategory) |  |  |
| `confidence` | [uint32](#uint32) |  | 0-100. |
| `body` | [string](#string) |  |  |


<a name="rill-v1-ReviewJob"></a>

### ReviewJob

What a reviewer is asked to review.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [string](#string) |  | `…/changes/{change}`. |
| `version` | [string](#string) |  | The version to review. |
| `reason` | [string](#string) |  | Why it was queued: `submitted`, `new version`, `requested by …`. |
| `profile` | [ReviewerProfile](#rill-v1-ReviewerProfile) |  |  |
| `prompt` | [string](#string) |  | The prompt file's text on trunk; empty when it does not exist. |
| `packet` | [ReviewPacket](#rill-v1-ReviewPacket) |  |  |


<a name="rill-v1-ReviewPacket"></a>

### ReviewPacket

The change as a reviewer sees it, within the server's context budget.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `description` | [string](#string) |  |  |
| `author` | [Principal](#rill-v1-Principal) |  |  |
| `risk` | [string](#string) |  | "trivial", "low", "normal", "sensitive" or "critical". |
| `intent_title` | [string](#string) |  |  |
| `intent_body` | [string](#string) |  |  |
| `files` | [ReviewFile](#rill-v1-ReviewFile) | repeated |  |
| `truncated` | [bool](#bool) |  | Diffs were dropped to fit the budget. |


<a name="rill-v1-ReviewRun"></a>

### ReviewRun

One reviewer's run (or one resolve) on one version of a change.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `reviewer` | [string](#string) |  | `agent:<name>`. |
| `version` | [string](#string) |  |  |
| `reason` | [string](#string) |  |  |
| `status` | [ReviewRunStatus](#rill-v1-ReviewRunStatus) |  |  |
| `queued_at` | [int64](#int64) |  |  |
| `finished_at` | [int64](#int64) |  |  |
| `findings` | [uint32](#uint32) |  | Findings posted. |
| `vote` | [int32](#int32) |  | The vote cast; 0 for none. |
| `model` | [string](#string) |  |  |
| `detail` | [string](#string) |  | The summary, or why it failed or was skipped. |
| `kind` | [string](#string) |  | "review" or "resolve". |


<a name="rill-v1-ReviewerStats"></a>

### ReviewerStats

One reviewer's record in a repository.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `reviewer` | [string](#string) |  | `agent:<name>`. |
| `trust` | [uint32](#uint32) |  | Permille: the mean of Beta(good + 1, bad + 1). |
| `outcomes` | [uint32](#uint32) |  |  |
| `approvals` | [uint64](#uint64) |  | Approved changes that landed. |
| `escapes` | [uint64](#uint64) |  | Of those, reverted, blamed for a red trunk or failed in audit. |
| `blocks` | [uint64](#uint64) |  | Blocking findings the author fixed. |
| `dismissed` | [uint64](#uint64) |  | Blocking findings resolved without a change. |
| `demoted` | [bool](#bool) |  | Routed nothing; its approvals do not count. |
| `note` | [string](#string) |  | Why it was demoted or reinstated last, and when. |
| `since` | [int64](#int64) |  |  |



<a name="rill-v1-ReviewRunStatus"></a>

### ReviewRunStatus



| Name | Number | Description |
| --- | --- | --- |
| `REVIEW_RUN_STATUS_UNSPECIFIED` | 0 |  |
| `REVIEW_RUN_STATUS_QUEUED` | 1 |  |
| `REVIEW_RUN_STATUS_DONE` | 2 |  |
| `REVIEW_RUN_STATUS_FAILED` | 3 |  |
| `REVIEW_RUN_STATUS_STALE` | 4 | The change had a new version before the result came. |
| `REVIEW_RUN_STATUS_SKIPPED` | 5 | Not run: the reviewer's daily budget was spent or it is not usable. |



## rill/v1/secrets.proto

Secrets: org secrets that verify plans give their steps as environment
variables. Values are write-only: no call returns one.

A secret is `orgs/{org}/secrets/{secret}`.

<a name="rill-v1-SecretService"></a>

### SecretService

Org secrets.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `UpdateSecret` | [UpdateSecretRequest](#rill-v1-UpdateSecretRequest) | [UpdateSecretResponse](#rill-v1-UpdateSecretResponse) | Set a secret, or with `allow_missing` create it (org administrators). |
| `DeleteSecret` | [DeleteSecretRequest](#rill-v1-DeleteSecretRequest) | [DeleteSecretResponse](#rill-v1-DeleteSecretResponse) | Remove a secret (org administrators). |
| `ListSecrets` | [ListSecretsRequest](#rill-v1-ListSecretsRequest) | [ListSecretsResponse](#rill-v1-ListSecretsResponse) | Names and scopes, never values. |



<a name="rill-v1-DeleteSecretRequest"></a>

### DeleteSecretRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `orgs/{org}/secrets/{secret}`. Required. |


<a name="rill-v1-DeleteSecretResponse"></a>

### DeleteSecretResponse




<a name="rill-v1-ListSecretsRequest"></a>

### ListSecretsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListSecretsResponse"></a>

### ListSecretsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `secrets` | [Secret](#rill-v1-Secret) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-Secret"></a>

### Secret

A secret, without its value.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `orgs/{org}/secrets/{secret}`. |
| `repos` | [string](#string) | repeated | Repositories whose plans may use it; empty is every repository. |
| `trunk_only` | [bool](#bool) |  | Only verification after landing gets it. |
| `updated_at` | [int64](#int64) |  | Output only. |
| `updated_by` | [Principal](#rill-v1-Principal) |  | Output only. |


<a name="rill-v1-UpdateSecretRequest"></a>

### UpdateSecretRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `secret` | [Secret](#rill-v1-Secret) |  | The secret, by `name`, with its scope. Required. |
| `value` | [bytes](#bytes) |  | The value; never returned. Required. |
| `allow_missing` | [bool](#bool) |  | Create it when absent. |


<a name="rill-v1-UpdateSecretResponse"></a>

### UpdateSecretResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `secret` | [Secret](#rill-v1-Secret) |  |  |




## rill/v1/session.proto

Signing in: exchanging a provider's token for a session, and refreshing
it. Open: callers have no token yet, and each RPC checks what it is
given.

<a name="rill-v1-SessionService"></a>

### SessionService

Signing in.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `ListSignInProviders` | [ListSignInProvidersRequest](#rill-v1-ListSignInProvidersRequest) | [ListSignInProvidersResponse](#rill-v1-ListSignInProvidersResponse) | The organization's providers a client can sign in through, with the endpoints of their device flow. |
| `ExchangeToken` | [ExchangeTokenRequest](#rill-v1-ExchangeTokenRequest) | [ExchangeTokenResponse](#rill-v1-ExchangeTokenResponse) | RFC 8693: a provider's ID token for a session. |
| `RefreshSession` | [RefreshSessionRequest](#rill-v1-RefreshSessionRequest) | [RefreshSessionResponse](#rill-v1-RefreshSessionResponse) | A refresh token for a new access token and a new refresh token. |



<a name="rill-v1-ExchangeTokenRequest"></a>

### ExchangeTokenRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `subject_token` | [string](#string) |  |  |
| `subject_token_type` | [string](#string) |  | `urn:ietf:params:oauth:token-type:id_token` (a person), or `urn:ietf:params:oauth:token-type:jwt` (a workload, for an agent). |
| `org` | [string](#string) |  | For workloads: the org whose trusted issuer signed the JWT. |
| `agent` | [string](#string) | optional | For workloads: the agent, when several bindings match. |
| `model` | [string](#string) | optional | For workloads: the model the agent runs as. |


<a name="rill-v1-ExchangeTokenResponse"></a>

### ExchangeTokenResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `grant` | [SessionGrant](#rill-v1-SessionGrant) |  |  |


<a name="rill-v1-ListSignInProvidersRequest"></a>

### ListSignInProvidersRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}`. Required. |


<a name="rill-v1-ListSignInProvidersResponse"></a>

### ListSignInProvidersResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `providers` | [SignInProvider](#rill-v1-SignInProvider) | repeated |  |


<a name="rill-v1-RefreshSessionRequest"></a>

### RefreshSessionRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `refresh_token` | [string](#string) |  |  |


<a name="rill-v1-RefreshSessionResponse"></a>

### RefreshSessionResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `grant` | [SessionGrant](#rill-v1-SessionGrant) |  |  |


<a name="rill-v1-SessionGrant"></a>

### SessionGrant



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `access_token` | [string](#string) |  |  |
| `expires_at` | [int64](#int64) |  |  |
| `refresh_token` | [string](#string) | optional |  |
| `session` | [string](#string) |  |  |
| `principal` | [Principal](#rill-v1-Principal) |  |  |
| `org` | [string](#string) |  |  |


<a name="rill-v1-SignInProvider"></a>

### SignInProvider



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `org` | [string](#string) |  |  |
| `name` | [string](#string) |  |  |
| `issuer` | [string](#string) |  |  |
| `cli_client_id` | [string](#string) | optional |  |
| `scopes` | [string](#string) | repeated |  |
| `device_authorization_endpoint` | [string](#string) | optional |  |
| `token_endpoint` | [string](#string) | optional |  |




## rill/v1/trunk.proto

Trunk: its head and queue, post-land verification, incidents, reverts
and bisection.

A landing is `orgs/{org}/repos/{repo}/trunk/positions/{n}`; an incident
is `orgs/{org}/repos/{repo}/incidents/{culprit position}`.

<a name="rill-v1-TrunkService"></a>

### TrunkService

Trunk status, verification and healing.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `GetTrunkStatus` | [GetTrunkStatusRequest](#rill-v1-GetTrunkStatusRequest) | [GetTrunkStatusResponse](#rill-v1-GetTrunkStatusResponse) | Trunk's head, what is landing and the last verified position. |
| `ListLandings` | [ListLandingsRequest](#rill-v1-ListLandingsRequest) | [ListLandingsResponse](#rill-v1-ListLandingsResponse) | Landings newest first, each with its verification. |
| `GetVerification` | [GetVerificationRequest](#rill-v1-GetVerificationRequest) | [GetVerificationResponse](#rill-v1-GetVerificationResponse) | A landing's verification: the post-land verifier's and runners' reports. |
| `ReportVerification` | [ReportVerificationRequest](#rill-v1-ReportVerificationRequest) | [ReportVerificationResponse](#rill-v1-ReportVerificationResponse) | File a runner's verification report for a landing. |
| `RevertLanding` | [RevertLandingRequest](#rill-v1-RevertLandingRequest) | [RevertLandingResponse](#rill-v1-RevertLandingResponse) | Land the revert of one landing. |
| `PreviewBulkRevert` | [PreviewBulkRevertRequest](#rill-v1-PreviewBulkRevertRequest) | [PreviewBulkRevertResponse](#rill-v1-PreviewBulkRevertResponse) | What reverting every landing a session, model, sponsor, author or intent made would revert, and which reverts conflict (owners). |
| `BulkRevert` | [BulkRevertRequest](#rill-v1-BulkRevertRequest) | [BulkRevertResponse](#rill-v1-BulkRevertResponse) | Queue those reverts in the emergency lane (owners). |
| `Bisect` | [BisectRequest](#rill-v1-BisectRequest) | [BisectResponse](#rill-v1-BisectResponse) | Find the landing in (from, to] that broke trunk. |
| `GetTrunkHealth` | [GetTrunkHealthRequest](#rill-v1-GetTrunkHealthRequest) | [GetTrunkHealthResponse](#rill-v1-GetTrunkHealthResponse) | Trunk's state and the incidents keeping it red. |
| `ListIncidents` | [ListIncidentsRequest](#rill-v1-ListIncidentsRequest) | [ListIncidentsResponse](#rill-v1-ListIncidentsResponse) | Incidents, open and closed, newest culprit first. |
| `FreezeTrunk` | [FreezeTrunkRequest](#rill-v1-FreezeTrunkRequest) | [FreezeTrunkResponse](#rill-v1-FreezeTrunkResponse) | Freeze trunk so only emergency landings land (owners). |
| `UnfreezeTrunk` | [UnfreezeTrunkRequest](#rill-v1-UnfreezeTrunkRequest) | [UnfreezeTrunkResponse](#rill-v1-UnfreezeTrunkResponse) | Thaw a frozen trunk (owners). |



<a name="rill-v1-BisectRequest"></a>

### BisectRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `from` | [uint64](#uint64) |  | The last known-good position. |
| `to` | [uint64](#uint64) |  | The first known-bad position. |
| `target` | [string](#string) |  |  |


<a name="rill-v1-BisectResponse"></a>

### BisectResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `culprit` | [uint64](#uint64) | optional |  |
| `tested` | [Verification](#rill-v1-Verification) | repeated | The positions the bisect tested, with their outcomes. |


<a name="rill-v1-BulkRevertRequest"></a>

### BulkRevertRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `selector` | [RevertSelector](#rill-v1-RevertSelector) |  |  |
| `reason` | [string](#string) |  |  |


<a name="rill-v1-BulkRevertResponse"></a>

### BulkRevertResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `operation` | [Operation](#rill-v1-Operation) |  | Done once every revert is queued; its result lists them. |


<a name="rill-v1-FreezeTrunkRequest"></a>

### FreezeTrunkRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `reason` | [string](#string) |  |  |


<a name="rill-v1-FreezeTrunkResponse"></a>

### FreezeTrunkResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `health` | [TrunkHealth](#rill-v1-TrunkHealth) |  |  |


<a name="rill-v1-GetTrunkHealthRequest"></a>

### GetTrunkHealthRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |


<a name="rill-v1-GetTrunkHealthResponse"></a>

### GetTrunkHealthResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `health` | [TrunkHealth](#rill-v1-TrunkHealth) |  |  |
| `open` | [Incident](#rill-v1-Incident) | repeated |  |


<a name="rill-v1-GetTrunkStatusRequest"></a>

### GetTrunkStatusRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |


<a name="rill-v1-GetTrunkStatusResponse"></a>

### GetTrunkStatusResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `head_position` | [uint64](#uint64) |  |  |
| `head_hash` | [bytes](#bytes) |  |  |
| `in_flight` | [LandRequest](#rill-v1-LandRequest) | repeated | Queued and rebasing requests, in serve order. |
| `last_verified_position` | [uint64](#uint64) | optional | The latest position whose verification passed. |


<a name="rill-v1-GetVerificationRequest"></a>

### GetVerificationRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landing` | [string](#string) |  | `…/trunk/positions/{n}`. Required. |


<a name="rill-v1-GetVerificationResponse"></a>

### GetVerificationResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `verification` | [Verification](#rill-v1-Verification) |  |  |


<a name="rill-v1-Incident"></a>

### Incident

A landing that broke trunk, and what happened since.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/incidents/{culprit position}`. |
| `culprit` | [uint64](#uint64) |  |  |
| `version` | [bytes](#bytes) |  |  |
| `change_id` | [bytes](#bytes) |  |  |
| `failures` | [string](#string) | repeated |  |
| `steps` | [string](#string) | repeated |  |
| `details` | [string](#string) |  |  |
| `author` | [Principal](#rill-v1-Principal) |  |  |
| `session` | [string](#string) | optional |  |
| `model` | [string](#string) | optional |  |
| `sponsor` | [Principal](#rill-v1-Principal) | optional |  |
| `state` | [string](#string) |  | open, fixing or closed. |
| `opened_at` | [int64](#int64) |  |  |
| `updated_at` | [int64](#int64) |  |  |
| `fix` | [uint64](#uint64) | optional | The landing that reverted or fixed it. |
| `resolution` | [string](#string) | optional | reverted, fixed or recovered, and at which position. |
| `resolved_at` | [uint64](#uint64) | optional |  |
| `revert_blocked` | [string](#string) | optional |  |
| `assigned` | [string](#string) | optional |  |


<a name="rill-v1-ListIncidentsRequest"></a>

### ListIncidentsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `filter` | [string](#string) |  | On `state` (`open`, `fixing`, `closed`). |


<a name="rill-v1-ListIncidentsResponse"></a>

### ListIncidentsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `incidents` | [Incident](#rill-v1-Incident) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListLandingsRequest"></a>

### ListLandingsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListLandingsResponse"></a>

### ListLandingsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landings` | [Landing](#rill-v1-Landing) | repeated | Newest first. |
| `verifications` | [Verification](#rill-v1-Verification) | repeated | Parallel to `landings`. |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-PreviewBulkRevertRequest"></a>

### PreviewBulkRevertRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `selector` | [RevertSelector](#rill-v1-RevertSelector) |  |  |


<a name="rill-v1-PreviewBulkRevertResponse"></a>

### PreviewBulkRevertResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `entries` | [BulkRevertEntry](#rill-v1-BulkRevertEntry) | repeated | Newest first. |


<a name="rill-v1-ReportVerificationRequest"></a>

### ReportVerificationRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `landing` | [string](#string) |  | `…/trunk/positions/{n}`. Required. |
| `target` | [string](#string) |  | Required. |
| `status` | [VerificationStatus](#rill-v1-VerificationStatus) |  | Required. |
| `details` | [string](#string) | optional |  |
| `artifact` | [bytes](#bytes) | optional |  |


<a name="rill-v1-ReportVerificationResponse"></a>

### ReportVerificationResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `verification` | [Verification](#rill-v1-Verification) |  |  |


<a name="rill-v1-RevertLandingRequest"></a>

### RevertLandingRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `landing` | [string](#string) |  | `…/trunk/positions/{n}`. Required. |
| `reason` | [string](#string) |  |  |


<a name="rill-v1-RevertLandingResponse"></a>

### RevertLandingResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landing` | [Landing](#rill-v1-Landing) |  | The revert's own landing. |
| `version` | [ChangeVersion](#rill-v1-ChangeVersion) |  |  |


<a name="rill-v1-RevertSelector"></a>

### RevertSelector

Which landings a bulk revert covers; name at least one origin.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `session` | [string](#string) | optional |  |
| `model` | [string](#string) | optional |  |
| `sponsor` | [string](#string) | optional | By name (`ann`) or as shown (`agent:bot`). |
| `author` | [string](#string) | optional |  |
| `intent` | [string](#string) | optional | `orgs/{org}/repos/{repo}/intents/{intent}`. |
| `since` | [uint64](#uint64) | optional | Positions in (since, until]; all of trunk when absent. |
| `until` | [uint64](#uint64) | optional |  |


<a name="rill-v1-TrunkHealth"></a>

### TrunkHealth



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `state` | [string](#string) |  | green, red, fixing or guarded. |
| `since` | [int64](#int64) |  |  |
| `open` | [uint64](#uint64) | repeated | Culprits of the open incidents. |
| `failing_steps` | [string](#string) | repeated |  |
| `frozen` | [bool](#bool) |  |  |
| `frozen_reason` | [string](#string) | optional |  |


<a name="rill-v1-UnfreezeTrunkRequest"></a>

### UnfreezeTrunkRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |


<a name="rill-v1-UnfreezeTrunkResponse"></a>

### UnfreezeTrunkResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `health` | [TrunkHealth](#rill-v1-TrunkHealth) |  |  |


<a name="rill-v1-Verification"></a>

### Verification

A landing's verification.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `landing` | [string](#string) |  | `…/trunk/positions/{n}`. |
| `position` | [uint64](#uint64) |  |  |
| `status` | [VerificationStatus](#rill-v1-VerificationStatus) |  | The worst of the reports; pending when there are none. |
| `culprit` | [uint64](#uint64) | optional | The position that introduced the failure, when known. |
| `reports` | [VerificationReport](#rill-v1-VerificationReport) | repeated |  |


<a name="rill-v1-VerificationReport"></a>

### VerificationReport

One runner's report on one landing.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `target` | [string](#string) |  | The verification target or runner (`unit`, `lint`, …). |
| `status` | [VerificationStatus](#rill-v1-VerificationStatus) |  |  |
| `reporter` | [Principal](#rill-v1-Principal) |  |  |
| `reported_at` | [int64](#int64) |  |  |
| `details` | [string](#string) | optional |  |
| `artifact` | [bytes](#bytes) | optional | A content-addressed log or artifact, if the runner stored one. |



<a name="rill-v1-VerificationStatus"></a>

### VerificationStatus



| Name | Number | Description |
| --- | --- | --- |
| `VERIFICATION_STATUS_UNSPECIFIED` | 0 |  |
| `VERIFICATION_STATUS_PENDING` | 1 |  |
| `VERIFICATION_STATUS_RUNNING` | 2 |  |
| `VERIFICATION_STATUS_PASSED` | 3 |  |
| `VERIFICATION_STATUS_FAILED` | 4 |  |
| `VERIFICATION_STATUS_INHERITED` | 5 | Failed, and a later bisect found an earlier culprit. |
| `VERIFICATION_STATUS_SKIPPED` | 6 |  |



## rill/v1/workspace.proto

Server-side workspaces: a principal's working copy of a repository, with
an operation log that can be undone.

A workspace is `orgs/{org}/repos/{repo}/workspaces/{workspace}`; an
entry of its log is `…/workspaces/{workspace}/operations/{operation}`.

<a name="rill-v1-WorkspaceService"></a>

### WorkspaceService

Workspaces and their operation logs.

| Method | Request | Response | Description |
| --- | --- | --- | --- |
| `CreateWorkspace` | [CreateWorkspaceRequest](#rill-v1-CreateWorkspaceRequest) | [CreateWorkspaceResponse](#rill-v1-CreateWorkspaceResponse) | Create a workspace on a base revision. |
| `GetWorkspace` | [GetWorkspaceRequest](#rill-v1-GetWorkspaceRequest) | [GetWorkspaceResponse](#rill-v1-GetWorkspaceResponse) | A workspace by name. |
| `ListWorkspaces` | [ListWorkspacesRequest](#rill-v1-ListWorkspacesRequest) | [ListWorkspacesResponse](#rill-v1-ListWorkspacesResponse) | A repository's workspaces. |
| `UpdateWorkspace` | [UpdateWorkspaceRequest](#rill-v1-UpdateWorkspaceRequest) | [UpdateWorkspaceResponse](#rill-v1-UpdateWorkspaceResponse) | Change a workspace's sparse profile, if it still has the `etag` given. |
| `DeleteWorkspace` | [DeleteWorkspaceRequest](#rill-v1-DeleteWorkspaceRequest) | [DeleteWorkspaceResponse](#rill-v1-DeleteWorkspaceResponse) | Delete a workspace, if it still has the `etag` given. |
| `SnapshotWorkspace` | [SnapshotWorkspaceRequest](#rill-v1-SnapshotWorkspaceRequest) | [SnapshotWorkspaceResponse](#rill-v1-SnapshotWorkspaceResponse) | Record the working copy's tree. |
| `StartChange` | [StartChangeRequest](#rill-v1-StartChangeRequest) | [StartChangeResponse](#rill-v1-StartChangeResponse) | Turn the snapshot into a version of the working change. |
| `SyncWorkspace` | [SyncWorkspaceRequest](#rill-v1-SyncWorkspaceRequest) | [SyncWorkspaceResponse](#rill-v1-SyncWorkspaceResponse) | Move the workspace to a new base, rebasing its working change. |
| `HandoffWorkspace` | [HandoffWorkspaceRequest](#rill-v1-HandoffWorkspaceRequest) | [HandoffWorkspaceResponse](#rill-v1-HandoffWorkspaceResponse) | Give the workspace to another principal. |
| `UndoOperation` | [UndoOperationRequest](#rill-v1-UndoOperationRequest) | [UndoOperationResponse](#rill-v1-UndoOperationResponse) | Restore the workspace to before an operation. |
| `ListWorkspaceOperations` | [ListWorkspaceOperationsRequest](#rill-v1-ListWorkspaceOperationsRequest) | [ListWorkspaceOperationsResponse](#rill-v1-ListWorkspaceOperationsResponse) | A workspace's operation log, newest first. |
| `DiffWorkspace` | [DiffWorkspaceRequest](#rill-v1-DiffWorkspaceRequest) | [DiffWorkspaceResponse](#rill-v1-DiffWorkspaceResponse) | The difference between the snapshot and the base. |



<a name="rill-v1-CreateWorkspaceRequest"></a>

### CreateWorkspaceRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `workspace` | [Workspace](#rill-v1-Workspace) |  | `display_name` and `sparse` are read; the rest is output only. |
| `base` | [string](#string) |  | A revision; trunk when empty. |


<a name="rill-v1-CreateWorkspaceResponse"></a>

### CreateWorkspaceResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `workspace` | [Workspace](#rill-v1-Workspace) |  |  |
| `operation` | [WorkspaceOperation](#rill-v1-WorkspaceOperation) |  |  |


<a name="rill-v1-DeleteWorkspaceRequest"></a>

### DeleteWorkspaceRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/workspaces/{workspace}`. Required. |
| `etag` | [string](#string) |  | The workspace's current etag; any when empty. |


<a name="rill-v1-DeleteWorkspaceResponse"></a>

### DeleteWorkspaceResponse




<a name="rill-v1-DiffWorkspaceRequest"></a>

### DiffWorkspaceRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/workspaces/{workspace}`. Required. |
| `paths` | [string](#string) | repeated |  |
| `format` | [DiffFormat](#rill-v1-DiffFormat) |  |  |
| `context_lines` | [uint32](#uint32) |  |  |


<a name="rill-v1-DiffWorkspaceResponse"></a>

### DiffWorkspaceResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `files` | [FileDiff](#rill-v1-FileDiff) | repeated |  |


<a name="rill-v1-GetWorkspaceRequest"></a>

### GetWorkspaceRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `name` | [string](#string) |  | `…/workspaces/{workspace}`. Required. |


<a name="rill-v1-GetWorkspaceResponse"></a>

### GetWorkspaceResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `workspace` | [Workspace](#rill-v1-Workspace) |  |  |


<a name="rill-v1-HandoffWorkspaceRequest"></a>

### HandoffWorkspaceRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/workspaces/{workspace}`. Required. |
| `to` | [Principal](#rill-v1-Principal) |  | Required. |


<a name="rill-v1-HandoffWorkspaceResponse"></a>

### HandoffWorkspaceResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `workspace` | [Workspace](#rill-v1-Workspace) |  |  |
| `operation` | [WorkspaceOperation](#rill-v1-WorkspaceOperation) |  |  |


<a name="rill-v1-ListWorkspaceOperationsRequest"></a>

### ListWorkspaceOperationsRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `…/workspaces/{workspace}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |


<a name="rill-v1-ListWorkspaceOperationsResponse"></a>

### ListWorkspaceOperationsResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `operations` | [WorkspaceOperation](#rill-v1-WorkspaceOperation) | repeated | Newest first. |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-ListWorkspacesRequest"></a>

### ListWorkspacesRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `parent` | [string](#string) |  | `orgs/{org}/repos/{repo}`. Required. |
| `page_size` | [int32](#int32) |  |  |
| `page_token` | [string](#string) |  |  |
| `filter` | [string](#string) |  | On `owner` (a principal, `ann` or `agent:bot`) and `display_name`. |


<a name="rill-v1-ListWorkspacesResponse"></a>

### ListWorkspacesResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `workspaces` | [Workspace](#rill-v1-Workspace) | repeated |  |
| `next_page_token` | [string](#string) |  |  |


<a name="rill-v1-SnapshotWorkspaceRequest"></a>

### SnapshotWorkspaceRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/workspaces/{workspace}`. Required. |
| `tree_hash` | [bytes](#bytes) |  | Root tree of the working copy; its objects must already be uploaded. |
| `edits` | [WorkspaceEdits](#rill-v1-WorkspaceEdits) |  | Edits the server applies to build the tree. |
| `etag` | [string](#string) |  | The workspace's current etag; any when empty. |


<a name="rill-v1-SnapshotWorkspaceResponse"></a>

### SnapshotWorkspaceResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `operation` | [WorkspaceOperation](#rill-v1-WorkspaceOperation) |  |  |


<a name="rill-v1-StartChangeRequest"></a>

### StartChangeRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/workspaces/{workspace}`. Required. |
| `message` | [string](#string) |  |  |
| `intent` | [string](#string) |  | `orgs/{org}/repos/{repo}/intents/{intent}`. |
| `provenance` | [Provenance](#rill-v1-Provenance) |  |  |


<a name="rill-v1-StartChangeResponse"></a>

### StartChangeResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `change` | [string](#string) |  | `…/changes/{change}`. |
| `info` | [ChangeInfo](#rill-v1-ChangeInfo) |  |  |
| `version` | [ChangeVersion](#rill-v1-ChangeVersion) |  |  |
| `operation` | [WorkspaceOperation](#rill-v1-WorkspaceOperation) |  |  |


<a name="rill-v1-SyncWorkspaceRequest"></a>

### SyncWorkspaceRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | `…/workspaces/{workspace}`. Required. |
| `base` | [string](#string) |  | A revision; trunk when empty. |


<a name="rill-v1-SyncWorkspaceResponse"></a>

### SyncWorkspaceResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `workspace` | [Workspace](#rill-v1-Workspace) |  |  |
| `operation` | [WorkspaceOperation](#rill-v1-WorkspaceOperation) |  |  |
| `info` | [ChangeInfo](#rill-v1-ChangeInfo) |  | The rebased working change, if one existed. |
| `version` | [ChangeVersion](#rill-v1-ChangeVersion) |  |  |


<a name="rill-v1-UndoOperationRequest"></a>

### UndoOperationRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `name` | [string](#string) |  | The operation to undo: `…/workspaces/{workspace}/operations/{operation}`, or `…/workspaces/{workspace}/operations/head`. Required. |


<a name="rill-v1-UndoOperationResponse"></a>

### UndoOperationResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `workspace` | [Workspace](#rill-v1-Workspace) |  |  |
| `operation` | [WorkspaceOperation](#rill-v1-WorkspaceOperation) |  |  |


<a name="rill-v1-UpdateWorkspaceRequest"></a>

### UpdateWorkspaceRequest



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `request_id` | [string](#string) |  |  |
| `workspace` | [Workspace](#rill-v1-Workspace) |  | `name` and `sparse` are read. Required. |
| `etag` | [string](#string) |  | The workspace's current etag. Required. |


<a name="rill-v1-UpdateWorkspaceResponse"></a>

### UpdateWorkspaceResponse



| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `workspace` | [Workspace](#rill-v1-Workspace) |  |  |
| `operation` | [WorkspaceOperation](#rill-v1-WorkspaceOperation) |  |  |


<a name="rill-v1-WorkspaceEdits"></a>

### WorkspaceEdits

Edits to a workspace's working copy. Patches are not accepted.

| Field | Type | Label | Description |
| --- | --- | --- | --- |
| `from_base` | [bool](#bool) |  | Apply to the base, replacing the snapshot, rather than to the current snapshot (the base when there is none). |
| `edits` | [Edit](#rill-v1-Edit) | repeated |  |




## Scalar value types

| Proto | Notes |
| --- | --- |
| <a name="double"></a> `double` |  |
| <a name="float"></a> `float` |  |
| <a name="int32"></a> `int32` | Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint32 instead. |
| <a name="int64"></a> `int64` | Uses variable-length encoding. Inefficient for encoding negative numbers – if your field is likely to have negative values, use sint64 instead. |
| <a name="uint32"></a> `uint32` | Uses variable-length encoding. |
| <a name="uint64"></a> `uint64` | Uses variable-length encoding. |
| <a name="sint32"></a> `sint32` | Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int32s. |
| <a name="sint64"></a> `sint64` | Uses variable-length encoding. Signed int value. These more efficiently encode negative numbers than regular int64s. |
| <a name="fixed32"></a> `fixed32` | Always four bytes. More efficient than uint32 if values are often greater than 2^28. |
| <a name="fixed64"></a> `fixed64` | Always eight bytes. More efficient than uint64 if values are often greater than 2^56. |
| <a name="sfixed32"></a> `sfixed32` | Always four bytes. |
| <a name="sfixed64"></a> `sfixed64` | Always eight bytes. |
| <a name="bool"></a> `bool` |  |
| <a name="string"></a> `string` | A string must always contain UTF-8 encoded or 7-bit ASCII text. |
| <a name="bytes"></a> `bytes` | May contain any arbitrary sequence of bytes. |

---
← Previous: [API surface](api-surface.md) · [Index](../index.md) · Next: [Events and workers](events-and-workers.md) →
