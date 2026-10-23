from google.protobuf import field_mask_pb2 as _field_mask_pb2
from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class CreateIntentRequest(_message.Message):
    __slots__ = ("request_id", "parent", "title", "body", "kind", "parent_intent", "risk_override", "scope")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    TITLE_FIELD_NUMBER: _ClassVar[int]
    BODY_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    PARENT_INTENT_FIELD_NUMBER: _ClassVar[int]
    RISK_OVERRIDE_FIELD_NUMBER: _ClassVar[int]
    SCOPE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    title: str
    body: str
    kind: _types_pb2.IntentKind
    parent_intent: str
    risk_override: _types_pb2.RiskClass
    scope: _types_pb2.IntentScope
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., title: _Optional[str] = ..., body: _Optional[str] = ..., kind: _Optional[_Union[_types_pb2.IntentKind, str]] = ..., parent_intent: _Optional[str] = ..., risk_override: _Optional[_Union[_types_pb2.RiskClass, str]] = ..., scope: _Optional[_Union[_types_pb2.IntentScope, _Mapping]] = ...) -> None: ...

class CreateIntentResponse(_message.Message):
    __slots__ = ("intent",)
    INTENT_FIELD_NUMBER: _ClassVar[int]
    intent: _types_pb2.Intent
    def __init__(self, intent: _Optional[_Union[_types_pb2.Intent, _Mapping]] = ...) -> None: ...

class GetIntentRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetIntentResponse(_message.Message):
    __slots__ = ("intent",)
    INTENT_FIELD_NUMBER: _ClassVar[int]
    intent: _types_pb2.Intent
    def __init__(self, intent: _Optional[_Union[_types_pb2.Intent, _Mapping]] = ...) -> None: ...

class UpdateIntentRequest(_message.Message):
    __slots__ = ("request_id", "intent", "update_mask", "etag")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    UPDATE_MASK_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    intent: _types_pb2.Intent
    update_mask: _field_mask_pb2.FieldMask
    etag: str
    def __init__(self, request_id: _Optional[str] = ..., intent: _Optional[_Union[_types_pb2.Intent, _Mapping]] = ..., update_mask: _Optional[_Union[_field_mask_pb2.FieldMask, _Mapping]] = ..., etag: _Optional[str] = ...) -> None: ...

class UpdateIntentResponse(_message.Message):
    __slots__ = ("intent",)
    INTENT_FIELD_NUMBER: _ClassVar[int]
    intent: _types_pb2.Intent
    def __init__(self, intent: _Optional[_Union[_types_pb2.Intent, _Mapping]] = ...) -> None: ...

class ListIntentsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token", "filter")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    FILTER_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    filter: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ..., filter: _Optional[str] = ...) -> None: ...

class ListIntentsResponse(_message.Message):
    __slots__ = ("intents", "next_page_token")
    INTENTS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    intents: _containers.RepeatedCompositeFieldContainer[_types_pb2.Intent]
    next_page_token: str
    def __init__(self, intents: _Optional[_Iterable[_Union[_types_pb2.Intent, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class LinkChangeRequest(_message.Message):
    __slots__ = ("request_id", "name", "change")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    change: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., change: _Optional[str] = ...) -> None: ...

class LinkChangeResponse(_message.Message):
    __slots__ = ("intent",)
    INTENT_FIELD_NUMBER: _ClassVar[int]
    intent: _types_pb2.Intent
    def __init__(self, intent: _Optional[_Union[_types_pb2.Intent, _Mapping]] = ...) -> None: ...

class FindSimilarIntentsRequest(_message.Message):
    __slots__ = ("parent", "title", "body", "max_results")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    TITLE_FIELD_NUMBER: _ClassVar[int]
    BODY_FIELD_NUMBER: _ClassVar[int]
    MAX_RESULTS_FIELD_NUMBER: _ClassVar[int]
    parent: str
    title: str
    body: str
    max_results: int
    def __init__(self, parent: _Optional[str] = ..., title: _Optional[str] = ..., body: _Optional[str] = ..., max_results: _Optional[int] = ...) -> None: ...

class ScoredIntent(_message.Message):
    __slots__ = ("intent", "score")
    INTENT_FIELD_NUMBER: _ClassVar[int]
    SCORE_FIELD_NUMBER: _ClassVar[int]
    intent: _types_pb2.Intent
    score: float
    def __init__(self, intent: _Optional[_Union[_types_pb2.Intent, _Mapping]] = ..., score: _Optional[float] = ...) -> None: ...

class FindSimilarIntentsResponse(_message.Message):
    __slots__ = ("results",)
    RESULTS_FIELD_NUMBER: _ClassVar[int]
    results: _containers.RepeatedCompositeFieldContainer[ScoredIntent]
    def __init__(self, results: _Optional[_Iterable[_Union[ScoredIntent, _Mapping]]] = ...) -> None: ...

class ApproveIntentRequest(_message.Message):
    __slots__ = ("request_id", "name", "revision")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    revision: int
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., revision: _Optional[int] = ...) -> None: ...

class ApproveIntentResponse(_message.Message):
    __slots__ = ("intent",)
    INTENT_FIELD_NUMBER: _ClassVar[int]
    intent: _types_pb2.Intent
    def __init__(self, intent: _Optional[_Union[_types_pb2.Intent, _Mapping]] = ...) -> None: ...

class RevokeIntentApprovalRequest(_message.Message):
    __slots__ = ("request_id", "name")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ...) -> None: ...

class RevokeIntentApprovalResponse(_message.Message):
    __slots__ = ("intent",)
    INTENT_FIELD_NUMBER: _ClassVar[int]
    intent: _types_pb2.Intent
    def __init__(self, intent: _Optional[_Union[_types_pb2.Intent, _Mapping]] = ...) -> None: ...

class ExplainIntentScopeRequest(_message.Message):
    __slots__ = ("change",)
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    change: str
    def __init__(self, change: _Optional[str] = ...) -> None: ...

class ExplainIntentScopeResponse(_message.Message):
    __slots__ = ("intent", "approved", "in_scope", "reasons", "missing_approvals")
    INTENT_FIELD_NUMBER: _ClassVar[int]
    APPROVED_FIELD_NUMBER: _ClassVar[int]
    IN_SCOPE_FIELD_NUMBER: _ClassVar[int]
    REASONS_FIELD_NUMBER: _ClassVar[int]
    MISSING_APPROVALS_FIELD_NUMBER: _ClassVar[int]
    intent: str
    approved: bool
    in_scope: bool
    reasons: _containers.RepeatedScalarFieldContainer[str]
    missing_approvals: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, intent: _Optional[str] = ..., approved: _Optional[bool] = ..., in_scope: _Optional[bool] = ..., reasons: _Optional[_Iterable[str]] = ..., missing_approvals: _Optional[_Iterable[str]] = ...) -> None: ...
