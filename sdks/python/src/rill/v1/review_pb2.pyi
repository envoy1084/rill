from rill.v1 import landing_pb2 as _landing_pb2
from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class GetReviewRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetReviewResponse(_message.Message):
    __slots__ = ("review", "labels")
    REVIEW_FIELD_NUMBER: _ClassVar[int]
    LABELS_FIELD_NUMBER: _ClassVar[int]
    review: _types_pb2.ReviewState
    labels: _containers.RepeatedCompositeFieldContainer[_types_pb2.Label]
    def __init__(self, review: _Optional[_Union[_types_pb2.ReviewState, _Mapping]] = ..., labels: _Optional[_Iterable[_Union[_types_pb2.Label, _Mapping]]] = ...) -> None: ...

class BatchGetReviewsRequest(_message.Message):
    __slots__ = ("parent", "names")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    NAMES_FIELD_NUMBER: _ClassVar[int]
    parent: str
    names: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, parent: _Optional[str] = ..., names: _Optional[_Iterable[str]] = ...) -> None: ...

class BatchGetReviewsResponse(_message.Message):
    __slots__ = ("reviews",)
    REVIEWS_FIELD_NUMBER: _ClassVar[int]
    reviews: _containers.RepeatedCompositeFieldContainer[_types_pb2.ReviewState]
    def __init__(self, reviews: _Optional[_Iterable[_Union[_types_pb2.ReviewState, _Mapping]]] = ...) -> None: ...

class ListReviewsRequest(_message.Message):
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

class ListReviewsResponse(_message.Message):
    __slots__ = ("reviews", "next_page_token")
    REVIEWS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    reviews: _containers.RepeatedCompositeFieldContainer[_types_pb2.ReviewState]
    next_page_token: str
    def __init__(self, reviews: _Optional[_Iterable[_Union[_types_pb2.ReviewState, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class SubmitChangeRequest(_message.Message):
    __slots__ = ("request_id", "change")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    change: str
    def __init__(self, request_id: _Optional[str] = ..., change: _Optional[str] = ...) -> None: ...

class SubmitChangeResponse(_message.Message):
    __slots__ = ("review",)
    REVIEW_FIELD_NUMBER: _ClassVar[int]
    review: _types_pb2.ReviewState
    def __init__(self, review: _Optional[_Union[_types_pb2.ReviewState, _Mapping]] = ...) -> None: ...

class VoteRequest(_message.Message):
    __slots__ = ("request_id", "review", "label", "value", "reason", "version")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    REVIEW_FIELD_NUMBER: _ClassVar[int]
    LABEL_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    review: str
    label: str
    value: int
    reason: str
    version: str
    def __init__(self, request_id: _Optional[str] = ..., review: _Optional[str] = ..., label: _Optional[str] = ..., value: _Optional[int] = ..., reason: _Optional[str] = ..., version: _Optional[str] = ...) -> None: ...

class VoteResponse(_message.Message):
    __slots__ = ("review",)
    REVIEW_FIELD_NUMBER: _ClassVar[int]
    review: _types_pb2.ReviewState
    def __init__(self, review: _Optional[_Union[_types_pb2.ReviewState, _Mapping]] = ...) -> None: ...

class CreateThreadRequest(_message.Message):
    __slots__ = ("request_id", "parent", "anchor", "body", "category", "confidence", "to")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    ANCHOR_FIELD_NUMBER: _ClassVar[int]
    BODY_FIELD_NUMBER: _ClassVar[int]
    CATEGORY_FIELD_NUMBER: _ClassVar[int]
    CONFIDENCE_FIELD_NUMBER: _ClassVar[int]
    TO_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    anchor: _types_pb2.ThreadAnchor
    body: str
    category: _types_pb2.CommentCategory
    confidence: int
    to: _types_pb2.Principal
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., anchor: _Optional[_Union[_types_pb2.ThreadAnchor, _Mapping]] = ..., body: _Optional[str] = ..., category: _Optional[_Union[_types_pb2.CommentCategory, str]] = ..., confidence: _Optional[int] = ..., to: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ...) -> None: ...

class CreateThreadResponse(_message.Message):
    __slots__ = ("thread",)
    THREAD_FIELD_NUMBER: _ClassVar[int]
    thread: _types_pb2.Thread
    def __init__(self, thread: _Optional[_Union[_types_pb2.Thread, _Mapping]] = ...) -> None: ...

class ReplyThreadRequest(_message.Message):
    __slots__ = ("request_id", "name", "body", "category", "confidence")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    BODY_FIELD_NUMBER: _ClassVar[int]
    CATEGORY_FIELD_NUMBER: _ClassVar[int]
    CONFIDENCE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    body: str
    category: _types_pb2.CommentCategory
    confidence: int
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., body: _Optional[str] = ..., category: _Optional[_Union[_types_pb2.CommentCategory, str]] = ..., confidence: _Optional[int] = ...) -> None: ...

class ReplyThreadResponse(_message.Message):
    __slots__ = ("thread",)
    THREAD_FIELD_NUMBER: _ClassVar[int]
    thread: _types_pb2.Thread
    def __init__(self, thread: _Optional[_Union[_types_pb2.Thread, _Mapping]] = ...) -> None: ...

class ResolveThreadRequest(_message.Message):
    __slots__ = ("request_id", "name", "resolved")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    RESOLVED_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    resolved: bool
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., resolved: _Optional[bool] = ...) -> None: ...

class ResolveThreadResponse(_message.Message):
    __slots__ = ("thread",)
    THREAD_FIELD_NUMBER: _ClassVar[int]
    thread: _types_pb2.Thread
    def __init__(self, thread: _Optional[_Union[_types_pb2.Thread, _Mapping]] = ...) -> None: ...

class UpdateStackRequest(_message.Message):
    __slots__ = ("request_id", "parent", "changes")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    CHANGES_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    changes: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., changes: _Optional[_Iterable[str]] = ...) -> None: ...

class UpdateStackResponse(_message.Message):
    __slots__ = ("reviews",)
    REVIEWS_FIELD_NUMBER: _ClassVar[int]
    reviews: _containers.RepeatedCompositeFieldContainer[_types_pb2.ReviewState]
    def __init__(self, reviews: _Optional[_Iterable[_Union[_types_pb2.ReviewState, _Mapping]]] = ...) -> None: ...

class EvaluateLandingRequest(_message.Message):
    __slots__ = ("change",)
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    change: str
    def __init__(self, change: _Optional[str] = ...) -> None: ...

class SubmitDecision(_message.Message):
    __slots__ = ("allowed", "reasons", "matched_policies", "risk", "risk_reasons")
    ALLOWED_FIELD_NUMBER: _ClassVar[int]
    REASONS_FIELD_NUMBER: _ClassVar[int]
    MATCHED_POLICIES_FIELD_NUMBER: _ClassVar[int]
    RISK_FIELD_NUMBER: _ClassVar[int]
    RISK_REASONS_FIELD_NUMBER: _ClassVar[int]
    allowed: bool
    reasons: _containers.RepeatedScalarFieldContainer[str]
    matched_policies: _containers.RepeatedScalarFieldContainer[str]
    risk: _types_pb2.RiskClass
    risk_reasons: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, allowed: _Optional[bool] = ..., reasons: _Optional[_Iterable[str]] = ..., matched_policies: _Optional[_Iterable[str]] = ..., risk: _Optional[_Union[_types_pb2.RiskClass, str]] = ..., risk_reasons: _Optional[_Iterable[str]] = ...) -> None: ...

class EvaluateLandingResponse(_message.Message):
    __slots__ = ("decision",)
    DECISION_FIELD_NUMBER: _ClassVar[int]
    decision: SubmitDecision
    def __init__(self, decision: _Optional[_Union[SubmitDecision, _Mapping]] = ...) -> None: ...

class AbandonChangeRequest(_message.Message):
    __slots__ = ("request_id", "change", "reason")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    change: str
    reason: str
    def __init__(self, request_id: _Optional[str] = ..., change: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class AbandonChangeResponse(_message.Message):
    __slots__ = ("review",)
    REVIEW_FIELD_NUMBER: _ClassVar[int]
    review: _types_pb2.ReviewState
    def __init__(self, review: _Optional[_Union[_types_pb2.ReviewState, _Mapping]] = ...) -> None: ...

class WatchReviewRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class WatchReviewResponse(_message.Message):
    __slots__ = ("change_id", "at", "status_changed", "voted", "thread_updated", "new_version", "land_update")
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    AT_FIELD_NUMBER: _ClassVar[int]
    STATUS_CHANGED_FIELD_NUMBER: _ClassVar[int]
    VOTED_FIELD_NUMBER: _ClassVar[int]
    THREAD_UPDATED_FIELD_NUMBER: _ClassVar[int]
    NEW_VERSION_FIELD_NUMBER: _ClassVar[int]
    LAND_UPDATE_FIELD_NUMBER: _ClassVar[int]
    change_id: bytes
    at: int
    status_changed: _types_pb2.ReviewStatus
    voted: _types_pb2.LabelVote
    thread_updated: _types_pb2.Thread
    new_version: bytes
    land_update: _landing_pb2.LandRequest
    def __init__(self, change_id: _Optional[bytes] = ..., at: _Optional[int] = ..., status_changed: _Optional[_Union[_types_pb2.ReviewStatus, str]] = ..., voted: _Optional[_Union[_types_pb2.LabelVote, _Mapping]] = ..., thread_updated: _Optional[_Union[_types_pb2.Thread, _Mapping]] = ..., new_version: _Optional[bytes] = ..., land_update: _Optional[_Union[_landing_pb2.LandRequest, _Mapping]] = ...) -> None: ...
