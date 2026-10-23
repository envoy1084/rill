from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ObjectKind(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    OBJECT_KIND_UNSPECIFIED: _ClassVar[ObjectKind]
    OBJECT_KIND_BLOB: _ClassVar[ObjectKind]
    OBJECT_KIND_CHUNK: _ClassVar[ObjectKind]
    OBJECT_KIND_BLOB_MANIFEST: _ClassVar[ObjectKind]
    OBJECT_KIND_TREE: _ClassVar[ObjectKind]
    OBJECT_KIND_CHANGE_VERSION: _ClassVar[ObjectKind]
    OBJECT_KIND_DERIVED: _ClassVar[ObjectKind]

class PrincipalKind(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    PRINCIPAL_KIND_UNSPECIFIED: _ClassVar[PrincipalKind]
    PRINCIPAL_KIND_HUMAN: _ClassVar[PrincipalKind]
    PRINCIPAL_KIND_AGENT: _ClassVar[PrincipalKind]
    PRINCIPAL_KIND_SERVICE: _ClassVar[PrincipalKind]

class FileKind(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    FILE_KIND_UNSPECIFIED: _ClassVar[FileKind]
    FILE_KIND_REGULAR: _ClassVar[FileKind]
    FILE_KIND_EXECUTABLE: _ClassVar[FileKind]
    FILE_KIND_SYMLINK: _ClassVar[FileKind]

class RiskClass(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    RISK_CLASS_UNSPECIFIED: _ClassVar[RiskClass]
    RISK_CLASS_TRIVIAL: _ClassVar[RiskClass]
    RISK_CLASS_LOW: _ClassVar[RiskClass]
    RISK_CLASS_NORMAL: _ClassVar[RiskClass]
    RISK_CLASS_SENSITIVE: _ClassVar[RiskClass]
    RISK_CLASS_CRITICAL: _ClassVar[RiskClass]

class IntentKind(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    INTENT_KIND_UNSPECIFIED: _ClassVar[IntentKind]
    INTENT_KIND_FEATURE: _ClassVar[IntentKind]
    INTENT_KIND_BUG: _ClassVar[IntentKind]
    INTENT_KIND_REFACTOR: _ClassVar[IntentKind]
    INTENT_KIND_CHORE: _ClassVar[IntentKind]
    INTENT_KIND_EXPERIMENT: _ClassVar[IntentKind]
    INTENT_KIND_SYSTEM: _ClassVar[IntentKind]

class IntentStatus(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    INTENT_STATUS_UNSPECIFIED: _ClassVar[IntentStatus]
    INTENT_STATUS_OPEN: _ClassVar[IntentStatus]
    INTENT_STATUS_IN_PROGRESS: _ClassVar[IntentStatus]
    INTENT_STATUS_DONE: _ClassVar[IntentStatus]
    INTENT_STATUS_ABANDONED: _ClassVar[IntentStatus]

class CommentCategory(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    COMMENT_CATEGORY_UNSPECIFIED: _ClassVar[CommentCategory]
    COMMENT_CATEGORY_DISCUSSION: _ClassVar[CommentCategory]
    COMMENT_CATEGORY_BUG: _ClassVar[CommentCategory]
    COMMENT_CATEGORY_SECURITY: _ClassVar[CommentCategory]
    COMMENT_CATEGORY_STYLE: _ClassVar[CommentCategory]
    COMMENT_CATEGORY_TEST_GAP: _ClassVar[CommentCategory]
    COMMENT_CATEGORY_QUESTION: _ClassVar[CommentCategory]
    COMMENT_CATEGORY_SUGGESTION: _ClassVar[CommentCategory]

class ReviewStatus(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    REVIEW_STATUS_UNSPECIFIED: _ClassVar[ReviewStatus]
    REVIEW_STATUS_OPEN: _ClassVar[ReviewStatus]
    REVIEW_STATUS_LANDABLE: _ClassVar[ReviewStatus]
    REVIEW_STATUS_LANDING: _ClassVar[ReviewStatus]
    REVIEW_STATUS_LANDED: _ClassVar[ReviewStatus]
    REVIEW_STATUS_ABANDONED: _ClassVar[ReviewStatus]

class DiffFormat(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    DIFF_FORMAT_UNSPECIFIED: _ClassVar[DiffFormat]
    DIFF_FORMAT_STRUCTURED: _ClassVar[DiffFormat]
    DIFF_FORMAT_UNIFIED: _ClassVar[DiffFormat]
OBJECT_KIND_UNSPECIFIED: ObjectKind
OBJECT_KIND_BLOB: ObjectKind
OBJECT_KIND_CHUNK: ObjectKind
OBJECT_KIND_BLOB_MANIFEST: ObjectKind
OBJECT_KIND_TREE: ObjectKind
OBJECT_KIND_CHANGE_VERSION: ObjectKind
OBJECT_KIND_DERIVED: ObjectKind
PRINCIPAL_KIND_UNSPECIFIED: PrincipalKind
PRINCIPAL_KIND_HUMAN: PrincipalKind
PRINCIPAL_KIND_AGENT: PrincipalKind
PRINCIPAL_KIND_SERVICE: PrincipalKind
FILE_KIND_UNSPECIFIED: FileKind
FILE_KIND_REGULAR: FileKind
FILE_KIND_EXECUTABLE: FileKind
FILE_KIND_SYMLINK: FileKind
RISK_CLASS_UNSPECIFIED: RiskClass
RISK_CLASS_TRIVIAL: RiskClass
RISK_CLASS_LOW: RiskClass
RISK_CLASS_NORMAL: RiskClass
RISK_CLASS_SENSITIVE: RiskClass
RISK_CLASS_CRITICAL: RiskClass
INTENT_KIND_UNSPECIFIED: IntentKind
INTENT_KIND_FEATURE: IntentKind
INTENT_KIND_BUG: IntentKind
INTENT_KIND_REFACTOR: IntentKind
INTENT_KIND_CHORE: IntentKind
INTENT_KIND_EXPERIMENT: IntentKind
INTENT_KIND_SYSTEM: IntentKind
INTENT_STATUS_UNSPECIFIED: IntentStatus
INTENT_STATUS_OPEN: IntentStatus
INTENT_STATUS_IN_PROGRESS: IntentStatus
INTENT_STATUS_DONE: IntentStatus
INTENT_STATUS_ABANDONED: IntentStatus
COMMENT_CATEGORY_UNSPECIFIED: CommentCategory
COMMENT_CATEGORY_DISCUSSION: CommentCategory
COMMENT_CATEGORY_BUG: CommentCategory
COMMENT_CATEGORY_SECURITY: CommentCategory
COMMENT_CATEGORY_STYLE: CommentCategory
COMMENT_CATEGORY_TEST_GAP: CommentCategory
COMMENT_CATEGORY_QUESTION: CommentCategory
COMMENT_CATEGORY_SUGGESTION: CommentCategory
REVIEW_STATUS_UNSPECIFIED: ReviewStatus
REVIEW_STATUS_OPEN: ReviewStatus
REVIEW_STATUS_LANDABLE: ReviewStatus
REVIEW_STATUS_LANDING: ReviewStatus
REVIEW_STATUS_LANDED: ReviewStatus
REVIEW_STATUS_ABANDONED: ReviewStatus
DIFF_FORMAT_UNSPECIFIED: DiffFormat
DIFF_FORMAT_STRUCTURED: DiffFormat
DIFF_FORMAT_UNIFIED: DiffFormat

class ObjectHash(_message.Message):
    __slots__ = ("bytes",)
    BYTES_FIELD_NUMBER: _ClassVar[int]
    bytes: bytes
    def __init__(self, bytes: _Optional[bytes] = ...) -> None: ...

class Principal(_message.Message):
    __slots__ = ("id", "kind", "name", "identity")
    ID_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    IDENTITY_FIELD_NUMBER: _ClassVar[int]
    id: bytes
    kind: PrincipalKind
    name: str
    identity: str
    def __init__(self, id: _Optional[bytes] = ..., kind: _Optional[_Union[PrincipalKind, str]] = ..., name: _Optional[str] = ..., identity: _Optional[str] = ...) -> None: ...

class Provenance(_message.Message):
    __slots__ = ("tool", "model", "session", "prompt_ref", "sponsor", "resolved_by", "extra")
    class ExtraEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    TOOL_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    SESSION_FIELD_NUMBER: _ClassVar[int]
    PROMPT_REF_FIELD_NUMBER: _ClassVar[int]
    SPONSOR_FIELD_NUMBER: _ClassVar[int]
    RESOLVED_BY_FIELD_NUMBER: _ClassVar[int]
    EXTRA_FIELD_NUMBER: _ClassVar[int]
    tool: str
    model: str
    session: str
    prompt_ref: str
    sponsor: Principal
    resolved_by: str
    extra: _containers.ScalarMap[str, str]
    def __init__(self, tool: _Optional[str] = ..., model: _Optional[str] = ..., session: _Optional[str] = ..., prompt_ref: _Optional[str] = ..., sponsor: _Optional[_Union[Principal, _Mapping]] = ..., resolved_by: _Optional[str] = ..., extra: _Optional[_Mapping[str, str]] = ...) -> None: ...

class ContentRef(_message.Message):
    __slots__ = ("inline", "blob", "chunked")
    INLINE_FIELD_NUMBER: _ClassVar[int]
    BLOB_FIELD_NUMBER: _ClassVar[int]
    CHUNKED_FIELD_NUMBER: _ClassVar[int]
    inline: bytes
    blob: BlobRef
    chunked: BlobRef
    def __init__(self, inline: _Optional[bytes] = ..., blob: _Optional[_Union[BlobRef, _Mapping]] = ..., chunked: _Optional[_Union[BlobRef, _Mapping]] = ...) -> None: ...

class BlobRef(_message.Message):
    __slots__ = ("hash", "size")
    HASH_FIELD_NUMBER: _ClassVar[int]
    SIZE_FIELD_NUMBER: _ClassVar[int]
    hash: bytes
    size: int
    def __init__(self, hash: _Optional[bytes] = ..., size: _Optional[int] = ...) -> None: ...

class FileState(_message.Message):
    __slots__ = ("content", "kind")
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    content: ContentRef
    kind: FileKind
    def __init__(self, content: _Optional[_Union[ContentRef, _Mapping]] = ..., kind: _Optional[_Union[FileKind, str]] = ...) -> None: ...

class MergeTerms(_message.Message):
    __slots__ = ("terms",)
    TERMS_FIELD_NUMBER: _ClassVar[int]
    terms: _containers.RepeatedCompositeFieldContainer[FileState]
    def __init__(self, terms: _Optional[_Iterable[_Union[FileState, _Mapping]]] = ...) -> None: ...

class CopySource(_message.Message):
    __slots__ = ("path", "version")
    PATH_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    path: str
    version: bytes
    def __init__(self, path: _Optional[str] = ..., version: _Optional[bytes] = ...) -> None: ...

class FileChange(_message.Message):
    __slots__ = ("set", "delete", "conflict")
    class Set(_message.Message):
        __slots__ = ("content", "kind", "copy_from")
        CONTENT_FIELD_NUMBER: _ClassVar[int]
        KIND_FIELD_NUMBER: _ClassVar[int]
        COPY_FROM_FIELD_NUMBER: _ClassVar[int]
        content: ContentRef
        kind: FileKind
        copy_from: CopySource
        def __init__(self, content: _Optional[_Union[ContentRef, _Mapping]] = ..., kind: _Optional[_Union[FileKind, str]] = ..., copy_from: _Optional[_Union[CopySource, _Mapping]] = ...) -> None: ...
    class Delete(_message.Message):
        __slots__ = ()
        def __init__(self) -> None: ...
    SET_FIELD_NUMBER: _ClassVar[int]
    DELETE_FIELD_NUMBER: _ClassVar[int]
    CONFLICT_FIELD_NUMBER: _ClassVar[int]
    set: FileChange.Set
    delete: FileChange.Delete
    conflict: MergeTerms
    def __init__(self, set: _Optional[_Union[FileChange.Set, _Mapping]] = ..., delete: _Optional[_Union[FileChange.Delete, _Mapping]] = ..., conflict: _Optional[_Union[MergeTerms, _Mapping]] = ...) -> None: ...

class SubtreeChange(_message.Message):
    __slots__ = ("copy", "delete")
    class Copy(_message.Message):
        __slots__ = ("from_path", "from_version")
        FROM_PATH_FIELD_NUMBER: _ClassVar[int]
        FROM_VERSION_FIELD_NUMBER: _ClassVar[int]
        from_path: str
        from_version: bytes
        def __init__(self, from_path: _Optional[str] = ..., from_version: _Optional[bytes] = ...) -> None: ...
    class Delete(_message.Message):
        __slots__ = ()
        def __init__(self) -> None: ...
    COPY_FIELD_NUMBER: _ClassVar[int]
    DELETE_FIELD_NUMBER: _ClassVar[int]
    copy: SubtreeChange.Copy
    delete: SubtreeChange.Delete
    def __init__(self, copy: _Optional[_Union[SubtreeChange.Copy, _Mapping]] = ..., delete: _Optional[_Union[SubtreeChange.Delete, _Mapping]] = ...) -> None: ...

class ChangeVersion(_message.Message):
    __slots__ = ("change_id", "parents", "predecessor", "subtree_changes", "file_changes", "author", "provenance", "intent", "message", "created_at", "extra")
    class SubtreeChangesEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: SubtreeChange
        def __init__(self, key: _Optional[str] = ..., value: _Optional[_Union[SubtreeChange, _Mapping]] = ...) -> None: ...
    class FileChangesEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: FileChange
        def __init__(self, key: _Optional[str] = ..., value: _Optional[_Union[FileChange, _Mapping]] = ...) -> None: ...
    class ExtraEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: bytes
        def __init__(self, key: _Optional[str] = ..., value: _Optional[bytes] = ...) -> None: ...
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    PARENTS_FIELD_NUMBER: _ClassVar[int]
    PREDECESSOR_FIELD_NUMBER: _ClassVar[int]
    SUBTREE_CHANGES_FIELD_NUMBER: _ClassVar[int]
    FILE_CHANGES_FIELD_NUMBER: _ClassVar[int]
    AUTHOR_FIELD_NUMBER: _ClassVar[int]
    PROVENANCE_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    EXTRA_FIELD_NUMBER: _ClassVar[int]
    change_id: bytes
    parents: _containers.RepeatedScalarFieldContainer[bytes]
    predecessor: bytes
    subtree_changes: _containers.MessageMap[str, SubtreeChange]
    file_changes: _containers.MessageMap[str, FileChange]
    author: Principal
    provenance: Provenance
    intent: bytes
    message: str
    created_at: int
    extra: _containers.ScalarMap[str, bytes]
    def __init__(self, change_id: _Optional[bytes] = ..., parents: _Optional[_Iterable[bytes]] = ..., predecessor: _Optional[bytes] = ..., subtree_changes: _Optional[_Mapping[str, SubtreeChange]] = ..., file_changes: _Optional[_Mapping[str, FileChange]] = ..., author: _Optional[_Union[Principal, _Mapping]] = ..., provenance: _Optional[_Union[Provenance, _Mapping]] = ..., intent: _Optional[bytes] = ..., message: _Optional[str] = ..., created_at: _Optional[int] = ..., extra: _Optional[_Mapping[str, bytes]] = ...) -> None: ...

class ChangeInfo(_message.Message):
    __slots__ = ("name", "hash", "change_id", "parents", "predecessor", "author", "provenance", "intent", "message", "created_at", "files_changed", "has_conflicts", "trunk_seq")
    NAME_FIELD_NUMBER: _ClassVar[int]
    HASH_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    PARENTS_FIELD_NUMBER: _ClassVar[int]
    PREDECESSOR_FIELD_NUMBER: _ClassVar[int]
    AUTHOR_FIELD_NUMBER: _ClassVar[int]
    PROVENANCE_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    FILES_CHANGED_FIELD_NUMBER: _ClassVar[int]
    HAS_CONFLICTS_FIELD_NUMBER: _ClassVar[int]
    TRUNK_SEQ_FIELD_NUMBER: _ClassVar[int]
    name: str
    hash: bytes
    change_id: bytes
    parents: _containers.RepeatedScalarFieldContainer[bytes]
    predecessor: bytes
    author: Principal
    provenance: Provenance
    intent: bytes
    message: str
    created_at: int
    files_changed: int
    has_conflicts: bool
    trunk_seq: int
    def __init__(self, name: _Optional[str] = ..., hash: _Optional[bytes] = ..., change_id: _Optional[bytes] = ..., parents: _Optional[_Iterable[bytes]] = ..., predecessor: _Optional[bytes] = ..., author: _Optional[_Union[Principal, _Mapping]] = ..., provenance: _Optional[_Union[Provenance, _Mapping]] = ..., intent: _Optional[bytes] = ..., message: _Optional[str] = ..., created_at: _Optional[int] = ..., files_changed: _Optional[int] = ..., has_conflicts: _Optional[bool] = ..., trunk_seq: _Optional[int] = ...) -> None: ...

class Signature(_message.Message):
    __slots__ = ("signer", "public_key", "signature", "signed_at")
    SIGNER_FIELD_NUMBER: _ClassVar[int]
    PUBLIC_KEY_FIELD_NUMBER: _ClassVar[int]
    SIGNATURE_FIELD_NUMBER: _ClassVar[int]
    SIGNED_AT_FIELD_NUMBER: _ClassVar[int]
    signer: Principal
    public_key: bytes
    signature: bytes
    signed_at: int
    def __init__(self, signer: _Optional[_Union[Principal, _Mapping]] = ..., public_key: _Optional[bytes] = ..., signature: _Optional[bytes] = ..., signed_at: _Optional[int] = ...) -> None: ...

class DirSummary(_message.Message):
    __slots__ = ("file_count", "dir_count", "total_size", "max_depth")
    FILE_COUNT_FIELD_NUMBER: _ClassVar[int]
    DIR_COUNT_FIELD_NUMBER: _ClassVar[int]
    TOTAL_SIZE_FIELD_NUMBER: _ClassVar[int]
    MAX_DEPTH_FIELD_NUMBER: _ClassVar[int]
    file_count: int
    dir_count: int
    total_size: int
    max_depth: int
    def __init__(self, file_count: _Optional[int] = ..., dir_count: _Optional[int] = ..., total_size: _Optional[int] = ..., max_depth: _Optional[int] = ...) -> None: ...

class TreeEntry(_message.Message):
    __slots__ = ("file", "dir")
    class File(_message.Message):
        __slots__ = ("content", "kind")
        CONTENT_FIELD_NUMBER: _ClassVar[int]
        KIND_FIELD_NUMBER: _ClassVar[int]
        content: ContentRef
        kind: FileKind
        def __init__(self, content: _Optional[_Union[ContentRef, _Mapping]] = ..., kind: _Optional[_Union[FileKind, str]] = ...) -> None: ...
    class Dir(_message.Message):
        __slots__ = ("hash", "summary")
        HASH_FIELD_NUMBER: _ClassVar[int]
        SUMMARY_FIELD_NUMBER: _ClassVar[int]
        hash: bytes
        summary: DirSummary
        def __init__(self, hash: _Optional[bytes] = ..., summary: _Optional[_Union[DirSummary, _Mapping]] = ...) -> None: ...
    FILE_FIELD_NUMBER: _ClassVar[int]
    DIR_FIELD_NUMBER: _ClassVar[int]
    file: TreeEntry.File
    dir: TreeEntry.Dir
    def __init__(self, file: _Optional[_Union[TreeEntry.File, _Mapping]] = ..., dir: _Optional[_Union[TreeEntry.Dir, _Mapping]] = ...) -> None: ...

class Tree(_message.Message):
    __slots__ = ("entries",)
    class EntriesEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: TreeEntry
        def __init__(self, key: _Optional[str] = ..., value: _Optional[_Union[TreeEntry, _Mapping]] = ...) -> None: ...
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    entries: _containers.MessageMap[str, TreeEntry]
    def __init__(self, entries: _Optional[_Mapping[str, TreeEntry]] = ...) -> None: ...

class NamedTreeEntry(_message.Message):
    __slots__ = ("name", "entry")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ENTRY_FIELD_NUMBER: _ClassVar[int]
    name: str
    entry: TreeEntry
    def __init__(self, name: _Optional[str] = ..., entry: _Optional[_Union[TreeEntry, _Mapping]] = ...) -> None: ...

class Intent(_message.Message):
    __slots__ = ("name", "etag", "id", "title", "body", "kind", "status", "owner", "parent", "risk_override", "changes", "created_at", "updated_at", "scope", "revision", "approvals")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    ID_FIELD_NUMBER: _ClassVar[int]
    TITLE_FIELD_NUMBER: _ClassVar[int]
    BODY_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    OWNER_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    RISK_OVERRIDE_FIELD_NUMBER: _ClassVar[int]
    CHANGES_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    UPDATED_AT_FIELD_NUMBER: _ClassVar[int]
    SCOPE_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    APPROVALS_FIELD_NUMBER: _ClassVar[int]
    name: str
    etag: str
    id: bytes
    title: str
    body: str
    kind: IntentKind
    status: IntentStatus
    owner: Principal
    parent: bytes
    risk_override: RiskClass
    changes: _containers.RepeatedScalarFieldContainer[bytes]
    created_at: int
    updated_at: int
    scope: IntentScope
    revision: int
    approvals: _containers.RepeatedCompositeFieldContainer[IntentApproval]
    def __init__(self, name: _Optional[str] = ..., etag: _Optional[str] = ..., id: _Optional[bytes] = ..., title: _Optional[str] = ..., body: _Optional[str] = ..., kind: _Optional[_Union[IntentKind, str]] = ..., status: _Optional[_Union[IntentStatus, str]] = ..., owner: _Optional[_Union[Principal, _Mapping]] = ..., parent: _Optional[bytes] = ..., risk_override: _Optional[_Union[RiskClass, str]] = ..., changes: _Optional[_Iterable[bytes]] = ..., created_at: _Optional[int] = ..., updated_at: _Optional[int] = ..., scope: _Optional[_Union[IntentScope, _Mapping]] = ..., revision: _Optional[int] = ..., approvals: _Optional[_Iterable[_Union[IntentApproval, _Mapping]]] = ...) -> None: ...

class IntentScope(_message.Message):
    __slots__ = ("paths", "forbid_paths", "max_files_per_change", "max_changes", "expires", "agent_quorum", "human_sample_permille", "changes_per_day", "lands_per_day")
    PATHS_FIELD_NUMBER: _ClassVar[int]
    FORBID_PATHS_FIELD_NUMBER: _ClassVar[int]
    MAX_FILES_PER_CHANGE_FIELD_NUMBER: _ClassVar[int]
    MAX_CHANGES_FIELD_NUMBER: _ClassVar[int]
    EXPIRES_FIELD_NUMBER: _ClassVar[int]
    AGENT_QUORUM_FIELD_NUMBER: _ClassVar[int]
    HUMAN_SAMPLE_PERMILLE_FIELD_NUMBER: _ClassVar[int]
    CHANGES_PER_DAY_FIELD_NUMBER: _ClassVar[int]
    LANDS_PER_DAY_FIELD_NUMBER: _ClassVar[int]
    paths: _containers.RepeatedScalarFieldContainer[str]
    forbid_paths: _containers.RepeatedScalarFieldContainer[str]
    max_files_per_change: int
    max_changes: int
    expires: int
    agent_quorum: int
    human_sample_permille: int
    changes_per_day: int
    lands_per_day: int
    def __init__(self, paths: _Optional[_Iterable[str]] = ..., forbid_paths: _Optional[_Iterable[str]] = ..., max_files_per_change: _Optional[int] = ..., max_changes: _Optional[int] = ..., expires: _Optional[int] = ..., agent_quorum: _Optional[int] = ..., human_sample_permille: _Optional[int] = ..., changes_per_day: _Optional[int] = ..., lands_per_day: _Optional[int] = ...) -> None: ...

class IntentApproval(_message.Message):
    __slots__ = ("by", "at", "revision", "owns")
    BY_FIELD_NUMBER: _ClassVar[int]
    AT_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    OWNS_FIELD_NUMBER: _ClassVar[int]
    by: Principal
    at: int
    revision: int
    owns: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, by: _Optional[_Union[Principal, _Mapping]] = ..., at: _Optional[int] = ..., revision: _Optional[int] = ..., owns: _Optional[_Iterable[str]] = ...) -> None: ...

class Label(_message.Message):
    __slots__ = ("name", "min", "max", "copy_on_trivial_rebase", "max_requires_human")
    NAME_FIELD_NUMBER: _ClassVar[int]
    MIN_FIELD_NUMBER: _ClassVar[int]
    MAX_FIELD_NUMBER: _ClassVar[int]
    COPY_ON_TRIVIAL_REBASE_FIELD_NUMBER: _ClassVar[int]
    MAX_REQUIRES_HUMAN_FIELD_NUMBER: _ClassVar[int]
    name: str
    min: int
    max: int
    copy_on_trivial_rebase: bool
    max_requires_human: bool
    def __init__(self, name: _Optional[str] = ..., min: _Optional[int] = ..., max: _Optional[int] = ..., copy_on_trivial_rebase: _Optional[bool] = ..., max_requires_human: _Optional[bool] = ...) -> None: ...

class LabelVote(_message.Message):
    __slots__ = ("label", "value", "voter", "version", "voted_at", "reason")
    LABEL_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    VOTER_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    VOTED_AT_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    label: str
    value: int
    voter: Principal
    version: bytes
    voted_at: int
    reason: str
    def __init__(self, label: _Optional[str] = ..., value: _Optional[int] = ..., voter: _Optional[_Union[Principal, _Mapping]] = ..., version: _Optional[bytes] = ..., voted_at: _Optional[int] = ..., reason: _Optional[str] = ...) -> None: ...

class ThreadAnchor(_message.Message):
    __slots__ = ("change", "lines", "file")
    class Change(_message.Message):
        __slots__ = ()
        def __init__(self) -> None: ...
    class Lines(_message.Message):
        __slots__ = ("version", "path", "start", "end", "context_hash")
        VERSION_FIELD_NUMBER: _ClassVar[int]
        PATH_FIELD_NUMBER: _ClassVar[int]
        START_FIELD_NUMBER: _ClassVar[int]
        END_FIELD_NUMBER: _ClassVar[int]
        CONTEXT_HASH_FIELD_NUMBER: _ClassVar[int]
        version: bytes
        path: str
        start: int
        end: int
        context_hash: str
        def __init__(self, version: _Optional[bytes] = ..., path: _Optional[str] = ..., start: _Optional[int] = ..., end: _Optional[int] = ..., context_hash: _Optional[str] = ...) -> None: ...
    class File(_message.Message):
        __slots__ = ("version", "path")
        VERSION_FIELD_NUMBER: _ClassVar[int]
        PATH_FIELD_NUMBER: _ClassVar[int]
        version: bytes
        path: str
        def __init__(self, version: _Optional[bytes] = ..., path: _Optional[str] = ...) -> None: ...
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    LINES_FIELD_NUMBER: _ClassVar[int]
    FILE_FIELD_NUMBER: _ClassVar[int]
    change: ThreadAnchor.Change
    lines: ThreadAnchor.Lines
    file: ThreadAnchor.File
    def __init__(self, change: _Optional[_Union[ThreadAnchor.Change, _Mapping]] = ..., lines: _Optional[_Union[ThreadAnchor.Lines, _Mapping]] = ..., file: _Optional[_Union[ThreadAnchor.File, _Mapping]] = ...) -> None: ...

class Comment(_message.Message):
    __slots__ = ("author", "body", "created_at", "category", "confidence")
    AUTHOR_FIELD_NUMBER: _ClassVar[int]
    BODY_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    CATEGORY_FIELD_NUMBER: _ClassVar[int]
    CONFIDENCE_FIELD_NUMBER: _ClassVar[int]
    author: Principal
    body: str
    created_at: int
    category: CommentCategory
    confidence: int
    def __init__(self, author: _Optional[_Union[Principal, _Mapping]] = ..., body: _Optional[str] = ..., created_at: _Optional[int] = ..., category: _Optional[_Union[CommentCategory, str]] = ..., confidence: _Optional[int] = ...) -> None: ...

class Thread(_message.Message):
    __slots__ = ("name", "id", "anchor", "comments", "resolved", "to")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ID_FIELD_NUMBER: _ClassVar[int]
    ANCHOR_FIELD_NUMBER: _ClassVar[int]
    COMMENTS_FIELD_NUMBER: _ClassVar[int]
    RESOLVED_FIELD_NUMBER: _ClassVar[int]
    TO_FIELD_NUMBER: _ClassVar[int]
    name: str
    id: str
    anchor: ThreadAnchor
    comments: _containers.RepeatedCompositeFieldContainer[Comment]
    resolved: bool
    to: Principal
    def __init__(self, name: _Optional[str] = ..., id: _Optional[str] = ..., anchor: _Optional[_Union[ThreadAnchor, _Mapping]] = ..., comments: _Optional[_Iterable[_Union[Comment, _Mapping]]] = ..., resolved: _Optional[bool] = ..., to: _Optional[_Union[Principal, _Mapping]] = ...) -> None: ...

class ReviewState(_message.Message):
    __slots__ = ("name", "change_id", "versions", "status", "intent", "owner", "votes", "threads", "risk", "risk_reasons", "stack", "landed_seq", "created_at", "updated_at", "extra")
    class ExtraEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    NAME_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    VERSIONS_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    OWNER_FIELD_NUMBER: _ClassVar[int]
    VOTES_FIELD_NUMBER: _ClassVar[int]
    THREADS_FIELD_NUMBER: _ClassVar[int]
    RISK_FIELD_NUMBER: _ClassVar[int]
    RISK_REASONS_FIELD_NUMBER: _ClassVar[int]
    STACK_FIELD_NUMBER: _ClassVar[int]
    LANDED_SEQ_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    UPDATED_AT_FIELD_NUMBER: _ClassVar[int]
    EXTRA_FIELD_NUMBER: _ClassVar[int]
    name: str
    change_id: bytes
    versions: _containers.RepeatedScalarFieldContainer[bytes]
    status: ReviewStatus
    intent: bytes
    owner: Principal
    votes: _containers.RepeatedCompositeFieldContainer[LabelVote]
    threads: _containers.RepeatedCompositeFieldContainer[Thread]
    risk: RiskClass
    risk_reasons: _containers.RepeatedScalarFieldContainer[str]
    stack: _containers.RepeatedScalarFieldContainer[bytes]
    landed_seq: int
    created_at: int
    updated_at: int
    extra: _containers.ScalarMap[str, str]
    def __init__(self, name: _Optional[str] = ..., change_id: _Optional[bytes] = ..., versions: _Optional[_Iterable[bytes]] = ..., status: _Optional[_Union[ReviewStatus, str]] = ..., intent: _Optional[bytes] = ..., owner: _Optional[_Union[Principal, _Mapping]] = ..., votes: _Optional[_Iterable[_Union[LabelVote, _Mapping]]] = ..., threads: _Optional[_Iterable[_Union[Thread, _Mapping]]] = ..., risk: _Optional[_Union[RiskClass, str]] = ..., risk_reasons: _Optional[_Iterable[str]] = ..., stack: _Optional[_Iterable[bytes]] = ..., landed_seq: _Optional[int] = ..., created_at: _Optional[int] = ..., updated_at: _Optional[int] = ..., extra: _Optional[_Mapping[str, str]] = ...) -> None: ...

class SparseProfile(_message.Message):
    __slots__ = ("include", "exclude")
    INCLUDE_FIELD_NUMBER: _ClassVar[int]
    EXCLUDE_FIELD_NUMBER: _ClassVar[int]
    include: _containers.RepeatedScalarFieldContainer[str]
    exclude: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, include: _Optional[_Iterable[str]] = ..., exclude: _Optional[_Iterable[str]] = ...) -> None: ...

class Workspace(_message.Message):
    __slots__ = ("name", "uid", "owner", "display_name", "base", "working_change", "snapshot", "sparse", "head_op", "created_at", "updated_at", "etag", "repo_uid")
    NAME_FIELD_NUMBER: _ClassVar[int]
    UID_FIELD_NUMBER: _ClassVar[int]
    OWNER_FIELD_NUMBER: _ClassVar[int]
    DISPLAY_NAME_FIELD_NUMBER: _ClassVar[int]
    BASE_FIELD_NUMBER: _ClassVar[int]
    WORKING_CHANGE_FIELD_NUMBER: _ClassVar[int]
    SNAPSHOT_FIELD_NUMBER: _ClassVar[int]
    SPARSE_FIELD_NUMBER: _ClassVar[int]
    HEAD_OP_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    UPDATED_AT_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    REPO_UID_FIELD_NUMBER: _ClassVar[int]
    name: str
    uid: bytes
    owner: Principal
    display_name: str
    base: bytes
    working_change: bytes
    snapshot: bytes
    sparse: SparseProfile
    head_op: bytes
    created_at: int
    updated_at: int
    etag: str
    repo_uid: bytes
    def __init__(self, name: _Optional[str] = ..., uid: _Optional[bytes] = ..., owner: _Optional[_Union[Principal, _Mapping]] = ..., display_name: _Optional[str] = ..., base: _Optional[bytes] = ..., working_change: _Optional[bytes] = ..., snapshot: _Optional[bytes] = ..., sparse: _Optional[_Union[SparseProfile, _Mapping]] = ..., head_op: _Optional[bytes] = ..., created_at: _Optional[int] = ..., updated_at: _Optional[int] = ..., etag: _Optional[str] = ..., repo_uid: _Optional[bytes] = ...) -> None: ...

class OperationKind(_message.Message):
    __slots__ = ("create", "snapshot", "new_change", "describe", "rebase", "resolve", "set_sparse", "handoff", "undo", "other")
    class Create(_message.Message):
        __slots__ = ()
        def __init__(self) -> None: ...
    class Snapshot(_message.Message):
        __slots__ = ("tree",)
        TREE_FIELD_NUMBER: _ClassVar[int]
        tree: bytes
        def __init__(self, tree: _Optional[bytes] = ...) -> None: ...
    class NewChange(_message.Message):
        __slots__ = ("change", "base")
        CHANGE_FIELD_NUMBER: _ClassVar[int]
        BASE_FIELD_NUMBER: _ClassVar[int]
        change: bytes
        base: bytes
        def __init__(self, change: _Optional[bytes] = ..., base: _Optional[bytes] = ...) -> None: ...
    class Describe(_message.Message):
        __slots__ = ("version",)
        VERSION_FIELD_NUMBER: _ClassVar[int]
        version: bytes
        def __init__(self, version: _Optional[bytes] = ...) -> None: ...
    class Rebase(_message.Message):
        __slots__ = ("to",)
        FROM_FIELD_NUMBER: _ClassVar[int]
        TO_FIELD_NUMBER: _ClassVar[int]
        to: bytes
        def __init__(self, to: _Optional[bytes] = ..., **kwargs) -> None: ...
    class Resolve(_message.Message):
        __slots__ = ("version",)
        VERSION_FIELD_NUMBER: _ClassVar[int]
        version: bytes
        def __init__(self, version: _Optional[bytes] = ...) -> None: ...
    class SetSparse(_message.Message):
        __slots__ = ("profile",)
        PROFILE_FIELD_NUMBER: _ClassVar[int]
        profile: SparseProfile
        def __init__(self, profile: _Optional[_Union[SparseProfile, _Mapping]] = ...) -> None: ...
    class Handoff(_message.Message):
        __slots__ = ("to",)
        TO_FIELD_NUMBER: _ClassVar[int]
        to: Principal
        def __init__(self, to: _Optional[_Union[Principal, _Mapping]] = ...) -> None: ...
    class Undo(_message.Message):
        __slots__ = ("undone",)
        UNDONE_FIELD_NUMBER: _ClassVar[int]
        undone: bytes
        def __init__(self, undone: _Optional[bytes] = ...) -> None: ...
    class Other(_message.Message):
        __slots__ = ("description",)
        DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
        description: str
        def __init__(self, description: _Optional[str] = ...) -> None: ...
    CREATE_FIELD_NUMBER: _ClassVar[int]
    SNAPSHOT_FIELD_NUMBER: _ClassVar[int]
    NEW_CHANGE_FIELD_NUMBER: _ClassVar[int]
    DESCRIBE_FIELD_NUMBER: _ClassVar[int]
    REBASE_FIELD_NUMBER: _ClassVar[int]
    RESOLVE_FIELD_NUMBER: _ClassVar[int]
    SET_SPARSE_FIELD_NUMBER: _ClassVar[int]
    HANDOFF_FIELD_NUMBER: _ClassVar[int]
    UNDO_FIELD_NUMBER: _ClassVar[int]
    OTHER_FIELD_NUMBER: _ClassVar[int]
    create: OperationKind.Create
    snapshot: OperationKind.Snapshot
    new_change: OperationKind.NewChange
    describe: OperationKind.Describe
    rebase: OperationKind.Rebase
    resolve: OperationKind.Resolve
    set_sparse: OperationKind.SetSparse
    handoff: OperationKind.Handoff
    undo: OperationKind.Undo
    other: OperationKind.Other
    def __init__(self, create: _Optional[_Union[OperationKind.Create, _Mapping]] = ..., snapshot: _Optional[_Union[OperationKind.Snapshot, _Mapping]] = ..., new_change: _Optional[_Union[OperationKind.NewChange, _Mapping]] = ..., describe: _Optional[_Union[OperationKind.Describe, _Mapping]] = ..., rebase: _Optional[_Union[OperationKind.Rebase, _Mapping]] = ..., resolve: _Optional[_Union[OperationKind.Resolve, _Mapping]] = ..., set_sparse: _Optional[_Union[OperationKind.SetSparse, _Mapping]] = ..., handoff: _Optional[_Union[OperationKind.Handoff, _Mapping]] = ..., undo: _Optional[_Union[OperationKind.Undo, _Mapping]] = ..., other: _Optional[_Union[OperationKind.Other, _Mapping]] = ...) -> None: ...

class WorkspaceOperation(_message.Message):
    __slots__ = ("name", "id", "workspace", "parent", "actor", "kind", "at", "resulting")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ID_FIELD_NUMBER: _ClassVar[int]
    WORKSPACE_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    ACTOR_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    AT_FIELD_NUMBER: _ClassVar[int]
    RESULTING_FIELD_NUMBER: _ClassVar[int]
    name: str
    id: bytes
    workspace: bytes
    parent: bytes
    actor: Principal
    kind: OperationKind
    at: int
    resulting: Workspace
    def __init__(self, name: _Optional[str] = ..., id: _Optional[bytes] = ..., workspace: _Optional[bytes] = ..., parent: _Optional[bytes] = ..., actor: _Optional[_Union[Principal, _Mapping]] = ..., kind: _Optional[_Union[OperationKind, _Mapping]] = ..., at: _Optional[int] = ..., resulting: _Optional[_Union[Workspace, _Mapping]] = ...) -> None: ...

class Ref(_message.Message):
    __slots__ = ("name", "target", "trunk_position", "etag")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TARGET_FIELD_NUMBER: _ClassVar[int]
    TRUNK_POSITION_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    name: str
    target: bytes
    trunk_position: int
    etag: str
    def __init__(self, name: _Optional[str] = ..., target: _Optional[bytes] = ..., trunk_position: _Optional[int] = ..., etag: _Optional[str] = ...) -> None: ...

class Landing(_message.Message):
    __slots__ = ("name", "position", "version", "version_hash", "change_id", "landed_by", "landed_at", "reverts_position")
    NAME_FIELD_NUMBER: _ClassVar[int]
    POSITION_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    VERSION_HASH_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    LANDED_BY_FIELD_NUMBER: _ClassVar[int]
    LANDED_AT_FIELD_NUMBER: _ClassVar[int]
    REVERTS_POSITION_FIELD_NUMBER: _ClassVar[int]
    name: str
    position: int
    version: str
    version_hash: bytes
    change_id: bytes
    landed_by: Principal
    landed_at: int
    reverts_position: int
    def __init__(self, name: _Optional[str] = ..., position: _Optional[int] = ..., version: _Optional[str] = ..., version_hash: _Optional[bytes] = ..., change_id: _Optional[bytes] = ..., landed_by: _Optional[_Union[Principal, _Mapping]] = ..., landed_at: _Optional[int] = ..., reverts_position: _Optional[int] = ...) -> None: ...

class DiffLine(_message.Message):
    __slots__ = ("op", "text")
    class Op(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
        __slots__ = ()
        OP_UNSPECIFIED: _ClassVar[DiffLine.Op]
        OP_CONTEXT: _ClassVar[DiffLine.Op]
        OP_INSERT: _ClassVar[DiffLine.Op]
        OP_DELETE: _ClassVar[DiffLine.Op]
    OP_UNSPECIFIED: DiffLine.Op
    OP_CONTEXT: DiffLine.Op
    OP_INSERT: DiffLine.Op
    OP_DELETE: DiffLine.Op
    OP_FIELD_NUMBER: _ClassVar[int]
    TEXT_FIELD_NUMBER: _ClassVar[int]
    op: DiffLine.Op
    text: bytes
    def __init__(self, op: _Optional[_Union[DiffLine.Op, str]] = ..., text: _Optional[bytes] = ...) -> None: ...

class DiffHunk(_message.Message):
    __slots__ = ("before_start", "before_len", "after_start", "after_len", "lines")
    BEFORE_START_FIELD_NUMBER: _ClassVar[int]
    BEFORE_LEN_FIELD_NUMBER: _ClassVar[int]
    AFTER_START_FIELD_NUMBER: _ClassVar[int]
    AFTER_LEN_FIELD_NUMBER: _ClassVar[int]
    LINES_FIELD_NUMBER: _ClassVar[int]
    before_start: int
    before_len: int
    after_start: int
    after_len: int
    lines: _containers.RepeatedCompositeFieldContainer[DiffLine]
    def __init__(self, before_start: _Optional[int] = ..., before_len: _Optional[int] = ..., after_start: _Optional[int] = ..., after_len: _Optional[int] = ..., lines: _Optional[_Iterable[_Union[DiffLine, _Mapping]]] = ...) -> None: ...

class FileDiff(_message.Message):
    __slots__ = ("path", "before", "after", "hunks", "unified", "binary", "conflict")
    PATH_FIELD_NUMBER: _ClassVar[int]
    BEFORE_FIELD_NUMBER: _ClassVar[int]
    AFTER_FIELD_NUMBER: _ClassVar[int]
    HUNKS_FIELD_NUMBER: _ClassVar[int]
    UNIFIED_FIELD_NUMBER: _ClassVar[int]
    BINARY_FIELD_NUMBER: _ClassVar[int]
    CONFLICT_FIELD_NUMBER: _ClassVar[int]
    path: str
    before: FileState
    after: FileState
    hunks: _containers.RepeatedCompositeFieldContainer[DiffHunk]
    unified: str
    binary: bool
    conflict: MergeTerms
    def __init__(self, path: _Optional[str] = ..., before: _Optional[_Union[FileState, _Mapping]] = ..., after: _Optional[_Union[FileState, _Mapping]] = ..., hunks: _Optional[_Iterable[_Union[DiffHunk, _Mapping]]] = ..., unified: _Optional[str] = ..., binary: _Optional[bool] = ..., conflict: _Optional[_Union[MergeTerms, _Mapping]] = ...) -> None: ...

class Repo(_message.Message):
    __slots__ = ("name", "uid", "repo_id", "description", "created_at", "trunk_head_position", "windows_safe", "org")
    NAME_FIELD_NUMBER: _ClassVar[int]
    UID_FIELD_NUMBER: _ClassVar[int]
    REPO_ID_FIELD_NUMBER: _ClassVar[int]
    DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    TRUNK_HEAD_POSITION_FIELD_NUMBER: _ClassVar[int]
    WINDOWS_SAFE_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    name: str
    uid: bytes
    repo_id: str
    description: str
    created_at: int
    trunk_head_position: int
    windows_safe: bool
    org: str
    def __init__(self, name: _Optional[str] = ..., uid: _Optional[bytes] = ..., repo_id: _Optional[str] = ..., description: _Optional[str] = ..., created_at: _Optional[int] = ..., trunk_head_position: _Optional[int] = ..., windows_safe: _Optional[bool] = ..., org: _Optional[str] = ...) -> None: ...

class Org(_message.Message):
    __slots__ = ("name", "org_id", "description", "created_at")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ORG_ID_FIELD_NUMBER: _ClassVar[int]
    DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    name: str
    org_id: str
    description: str
    created_at: int
    def __init__(self, name: _Optional[str] = ..., org_id: _Optional[str] = ..., description: _Optional[str] = ..., created_at: _Optional[int] = ...) -> None: ...
