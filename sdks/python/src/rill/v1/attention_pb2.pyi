from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class InboxKind(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    INBOX_KIND_UNSPECIFIED: _ClassVar[InboxKind]
    INBOX_KIND_ESCALATION: _ClassVar[InboxKind]
    INBOX_KIND_AUDIT: _ClassVar[InboxKind]
    INBOX_KIND_OWNER_REVIEW: _ClassVar[InboxKind]
    INBOX_KIND_INTENT_APPROVAL: _ClassVar[InboxKind]
    INBOX_KIND_THREAD: _ClassVar[InboxKind]
INBOX_KIND_UNSPECIFIED: InboxKind
INBOX_KIND_ESCALATION: InboxKind
INBOX_KIND_AUDIT: InboxKind
INBOX_KIND_OWNER_REVIEW: InboxKind
INBOX_KIND_INTENT_APPROVAL: InboxKind
INBOX_KIND_THREAD: InboxKind

class InboxItem(_message.Message):
    __slots__ = ("kind", "change", "intent", "landing", "title", "detail", "since", "urgency")
    KIND_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    LANDING_FIELD_NUMBER: _ClassVar[int]
    TITLE_FIELD_NUMBER: _ClassVar[int]
    DETAIL_FIELD_NUMBER: _ClassVar[int]
    SINCE_FIELD_NUMBER: _ClassVar[int]
    URGENCY_FIELD_NUMBER: _ClassVar[int]
    kind: InboxKind
    change: str
    intent: str
    landing: str
    title: str
    detail: str
    since: int
    urgency: int
    def __init__(self, kind: _Optional[_Union[InboxKind, str]] = ..., change: _Optional[str] = ..., intent: _Optional[str] = ..., landing: _Optional[str] = ..., title: _Optional[str] = ..., detail: _Optional[str] = ..., since: _Optional[int] = ..., urgency: _Optional[int] = ...) -> None: ...

class ListInboxRequest(_message.Message):
    __slots__ = ("parent", "limit")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    LIMIT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    limit: int
    def __init__(self, parent: _Optional[str] = ..., limit: _Optional[int] = ...) -> None: ...

class ListInboxResponse(_message.Message):
    __slots__ = ("items",)
    ITEMS_FIELD_NUMBER: _ClassVar[int]
    items: _containers.RepeatedCompositeFieldContainer[InboxItem]
    def __init__(self, items: _Optional[_Iterable[_Union[InboxItem, _Mapping]]] = ...) -> None: ...

class Audit(_message.Message):
    __slots__ = ("landing", "change", "version", "assignees", "risk", "status", "sampled_at", "decided_by", "decided_at", "note")
    LANDING_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    ASSIGNEES_FIELD_NUMBER: _ClassVar[int]
    RISK_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    SAMPLED_AT_FIELD_NUMBER: _ClassVar[int]
    DECIDED_BY_FIELD_NUMBER: _ClassVar[int]
    DECIDED_AT_FIELD_NUMBER: _ClassVar[int]
    NOTE_FIELD_NUMBER: _ClassVar[int]
    landing: str
    change: str
    version: str
    assignees: _containers.RepeatedScalarFieldContainer[str]
    risk: str
    status: str
    sampled_at: int
    decided_by: _types_pb2.Principal
    decided_at: int
    note: str
    def __init__(self, landing: _Optional[str] = ..., change: _Optional[str] = ..., version: _Optional[str] = ..., assignees: _Optional[_Iterable[str]] = ..., risk: _Optional[str] = ..., status: _Optional[str] = ..., sampled_at: _Optional[int] = ..., decided_by: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., decided_at: _Optional[int] = ..., note: _Optional[str] = ...) -> None: ...

class ListAuditsRequest(_message.Message):
    __slots__ = ("parent",)
    PARENT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    def __init__(self, parent: _Optional[str] = ...) -> None: ...

class ListAuditsResponse(_message.Message):
    __slots__ = ("audits",)
    AUDITS_FIELD_NUMBER: _ClassVar[int]
    audits: _containers.RepeatedCompositeFieldContainer[Audit]
    def __init__(self, audits: _Optional[_Iterable[_Union[Audit, _Mapping]]] = ...) -> None: ...

class RecordAuditRequest(_message.Message):
    __slots__ = ("request_id", "landing", "passed", "note", "revert")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    LANDING_FIELD_NUMBER: _ClassVar[int]
    PASSED_FIELD_NUMBER: _ClassVar[int]
    NOTE_FIELD_NUMBER: _ClassVar[int]
    REVERT_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    landing: str
    passed: bool
    note: str
    revert: bool
    def __init__(self, request_id: _Optional[str] = ..., landing: _Optional[str] = ..., passed: _Optional[bool] = ..., note: _Optional[str] = ..., revert: _Optional[bool] = ...) -> None: ...

class RecordAuditResponse(_message.Message):
    __slots__ = ("audit",)
    AUDIT_FIELD_NUMBER: _ClassVar[int]
    audit: Audit
    def __init__(self, audit: _Optional[_Union[Audit, _Mapping]] = ...) -> None: ...

class SimilarGroup(_message.Message):
    __slots__ = ("signature", "changes", "versions")
    SIGNATURE_FIELD_NUMBER: _ClassVar[int]
    CHANGES_FIELD_NUMBER: _ClassVar[int]
    VERSIONS_FIELD_NUMBER: _ClassVar[int]
    signature: str
    changes: _containers.RepeatedScalarFieldContainer[str]
    versions: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, signature: _Optional[str] = ..., changes: _Optional[_Iterable[str]] = ..., versions: _Optional[_Iterable[str]] = ...) -> None: ...

class ListSimilarChangesRequest(_message.Message):
    __slots__ = ("intent",)
    INTENT_FIELD_NUMBER: _ClassVar[int]
    intent: str
    def __init__(self, intent: _Optional[str] = ...) -> None: ...

class ListSimilarChangesResponse(_message.Message):
    __slots__ = ("groups",)
    GROUPS_FIELD_NUMBER: _ClassVar[int]
    groups: _containers.RepeatedCompositeFieldContainer[SimilarGroup]
    def __init__(self, groups: _Optional[_Iterable[_Union[SimilarGroup, _Mapping]]] = ...) -> None: ...

class BatchVoteRequest(_message.Message):
    __slots__ = ("request_id", "intent", "signature", "label", "value", "reason")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    SIGNATURE_FIELD_NUMBER: _ClassVar[int]
    LABEL_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    intent: str
    signature: str
    label: str
    value: int
    reason: str
    def __init__(self, request_id: _Optional[str] = ..., intent: _Optional[str] = ..., signature: _Optional[str] = ..., label: _Optional[str] = ..., value: _Optional[int] = ..., reason: _Optional[str] = ...) -> None: ...

class BatchVoteResult(_message.Message):
    __slots__ = ("change", "voted", "error")
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    VOTED_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    change: str
    voted: bool
    error: str
    def __init__(self, change: _Optional[str] = ..., voted: _Optional[bool] = ..., error: _Optional[str] = ...) -> None: ...

class BatchVoteResponse(_message.Message):
    __slots__ = ("results",)
    RESULTS_FIELD_NUMBER: _ClassVar[int]
    results: _containers.RepeatedCompositeFieldContainer[BatchVoteResult]
    def __init__(self, results: _Optional[_Iterable[_Union[BatchVoteResult, _Mapping]]] = ...) -> None: ...

class GetIntentDigestRequest(_message.Message):
    __slots__ = ("intent", "day")
    INTENT_FIELD_NUMBER: _ClassVar[int]
    DAY_FIELD_NUMBER: _ClassVar[int]
    intent: str
    day: str
    def __init__(self, intent: _Optional[str] = ..., day: _Optional[str] = ...) -> None: ...

class GetIntentDigestResponse(_message.Message):
    __slots__ = ("day", "landed", "reverted", "flagged", "sampled", "open")
    DAY_FIELD_NUMBER: _ClassVar[int]
    LANDED_FIELD_NUMBER: _ClassVar[int]
    REVERTED_FIELD_NUMBER: _ClassVar[int]
    FLAGGED_FIELD_NUMBER: _ClassVar[int]
    SAMPLED_FIELD_NUMBER: _ClassVar[int]
    OPEN_FIELD_NUMBER: _ClassVar[int]
    day: str
    landed: int
    reverted: int
    flagged: int
    sampled: int
    open: int
    def __init__(self, day: _Optional[str] = ..., landed: _Optional[int] = ..., reverted: _Optional[int] = ..., flagged: _Optional[int] = ..., sampled: _Optional[int] = ..., open: _Optional[int] = ...) -> None: ...
