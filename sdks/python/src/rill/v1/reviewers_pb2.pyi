from rill.v1 import identity_pb2 as _identity_pb2
from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ReviewRunStatus(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    REVIEW_RUN_STATUS_UNSPECIFIED: _ClassVar[ReviewRunStatus]
    REVIEW_RUN_STATUS_QUEUED: _ClassVar[ReviewRunStatus]
    REVIEW_RUN_STATUS_DONE: _ClassVar[ReviewRunStatus]
    REVIEW_RUN_STATUS_FAILED: _ClassVar[ReviewRunStatus]
    REVIEW_RUN_STATUS_STALE: _ClassVar[ReviewRunStatus]
    REVIEW_RUN_STATUS_SKIPPED: _ClassVar[ReviewRunStatus]
REVIEW_RUN_STATUS_UNSPECIFIED: ReviewRunStatus
REVIEW_RUN_STATUS_QUEUED: ReviewRunStatus
REVIEW_RUN_STATUS_DONE: ReviewRunStatus
REVIEW_RUN_STATUS_FAILED: ReviewRunStatus
REVIEW_RUN_STATUS_STALE: ReviewRunStatus
REVIEW_RUN_STATUS_SKIPPED: ReviewRunStatus

class LeaseReviewJobRequest(_message.Message):
    __slots__ = ("parent", "wait_ms")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    WAIT_MS_FIELD_NUMBER: _ClassVar[int]
    parent: str
    wait_ms: int
    def __init__(self, parent: _Optional[str] = ..., wait_ms: _Optional[int] = ...) -> None: ...

class LeaseReviewJobResponse(_message.Message):
    __slots__ = ("lease", "lease_until", "job")
    LEASE_FIELD_NUMBER: _ClassVar[int]
    LEASE_UNTIL_FIELD_NUMBER: _ClassVar[int]
    JOB_FIELD_NUMBER: _ClassVar[int]
    lease: bytes
    lease_until: int
    job: ReviewJob
    def __init__(self, lease: _Optional[bytes] = ..., lease_until: _Optional[int] = ..., job: _Optional[_Union[ReviewJob, _Mapping]] = ...) -> None: ...

class ReviewJob(_message.Message):
    __slots__ = ("change", "version", "reason", "profile", "prompt", "packet")
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    PROFILE_FIELD_NUMBER: _ClassVar[int]
    PROMPT_FIELD_NUMBER: _ClassVar[int]
    PACKET_FIELD_NUMBER: _ClassVar[int]
    change: str
    version: str
    reason: str
    profile: _identity_pb2.ReviewerProfile
    prompt: str
    packet: ReviewPacket
    def __init__(self, change: _Optional[str] = ..., version: _Optional[str] = ..., reason: _Optional[str] = ..., profile: _Optional[_Union[_identity_pb2.ReviewerProfile, _Mapping]] = ..., prompt: _Optional[str] = ..., packet: _Optional[_Union[ReviewPacket, _Mapping]] = ...) -> None: ...

class ReviewPacket(_message.Message):
    __slots__ = ("description", "author", "risk", "intent_title", "intent_body", "files", "truncated")
    DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
    AUTHOR_FIELD_NUMBER: _ClassVar[int]
    RISK_FIELD_NUMBER: _ClassVar[int]
    INTENT_TITLE_FIELD_NUMBER: _ClassVar[int]
    INTENT_BODY_FIELD_NUMBER: _ClassVar[int]
    FILES_FIELD_NUMBER: _ClassVar[int]
    TRUNCATED_FIELD_NUMBER: _ClassVar[int]
    description: str
    author: _types_pb2.Principal
    risk: str
    intent_title: str
    intent_body: str
    files: _containers.RepeatedCompositeFieldContainer[ReviewFile]
    truncated: bool
    def __init__(self, description: _Optional[str] = ..., author: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., risk: _Optional[str] = ..., intent_title: _Optional[str] = ..., intent_body: _Optional[str] = ..., files: _Optional[_Iterable[_Union[ReviewFile, _Mapping]]] = ..., truncated: _Optional[bool] = ...) -> None: ...

class ReviewFile(_message.Message):
    __slots__ = ("path", "unified", "added", "deleted")
    PATH_FIELD_NUMBER: _ClassVar[int]
    UNIFIED_FIELD_NUMBER: _ClassVar[int]
    ADDED_FIELD_NUMBER: _ClassVar[int]
    DELETED_FIELD_NUMBER: _ClassVar[int]
    path: str
    unified: str
    added: bool
    deleted: bool
    def __init__(self, path: _Optional[str] = ..., unified: _Optional[str] = ..., added: _Optional[bool] = ..., deleted: _Optional[bool] = ...) -> None: ...

class ReviewFinding(_message.Message):
    __slots__ = ("path", "start_line", "end_line", "category", "confidence", "body")
    PATH_FIELD_NUMBER: _ClassVar[int]
    START_LINE_FIELD_NUMBER: _ClassVar[int]
    END_LINE_FIELD_NUMBER: _ClassVar[int]
    CATEGORY_FIELD_NUMBER: _ClassVar[int]
    CONFIDENCE_FIELD_NUMBER: _ClassVar[int]
    BODY_FIELD_NUMBER: _ClassVar[int]
    path: str
    start_line: int
    end_line: int
    category: _types_pb2.CommentCategory
    confidence: int
    body: str
    def __init__(self, path: _Optional[str] = ..., start_line: _Optional[int] = ..., end_line: _Optional[int] = ..., category: _Optional[_Union[_types_pb2.CommentCategory, str]] = ..., confidence: _Optional[int] = ..., body: _Optional[str] = ...) -> None: ...

class RenewReviewLeaseRequest(_message.Message):
    __slots__ = ("parent", "lease")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    LEASE_FIELD_NUMBER: _ClassVar[int]
    parent: str
    lease: bytes
    def __init__(self, parent: _Optional[str] = ..., lease: _Optional[bytes] = ...) -> None: ...

class RenewReviewLeaseResponse(_message.Message):
    __slots__ = ("lease", "lease_until")
    LEASE_FIELD_NUMBER: _ClassVar[int]
    LEASE_UNTIL_FIELD_NUMBER: _ClassVar[int]
    lease: bytes
    lease_until: int
    def __init__(self, lease: _Optional[bytes] = ..., lease_until: _Optional[int] = ...) -> None: ...

class CompleteReviewJobRequest(_message.Message):
    __slots__ = ("request_id", "parent", "lease", "findings", "vote", "summary", "model")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    LEASE_FIELD_NUMBER: _ClassVar[int]
    FINDINGS_FIELD_NUMBER: _ClassVar[int]
    VOTE_FIELD_NUMBER: _ClassVar[int]
    SUMMARY_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    lease: bytes
    findings: _containers.RepeatedCompositeFieldContainer[ReviewFinding]
    vote: int
    summary: str
    model: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., lease: _Optional[bytes] = ..., findings: _Optional[_Iterable[_Union[ReviewFinding, _Mapping]]] = ..., vote: _Optional[int] = ..., summary: _Optional[str] = ..., model: _Optional[str] = ...) -> None: ...

class CompleteReviewJobResponse(_message.Message):
    __slots__ = ("run",)
    RUN_FIELD_NUMBER: _ClassVar[int]
    run: ReviewRun
    def __init__(self, run: _Optional[_Union[ReviewRun, _Mapping]] = ...) -> None: ...

class FailReviewJobRequest(_message.Message):
    __slots__ = ("request_id", "parent", "lease", "error", "retry")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    LEASE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    RETRY_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    lease: bytes
    error: str
    retry: bool
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., lease: _Optional[bytes] = ..., error: _Optional[str] = ..., retry: _Optional[bool] = ...) -> None: ...

class FailReviewJobResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class RequestReviewRequest(_message.Message):
    __slots__ = ("request_id", "change", "reviewers")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    REVIEWERS_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    change: str
    reviewers: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, request_id: _Optional[str] = ..., change: _Optional[str] = ..., reviewers: _Optional[_Iterable[str]] = ...) -> None: ...

class RequestReviewResponse(_message.Message):
    __slots__ = ("runs",)
    RUNS_FIELD_NUMBER: _ClassVar[int]
    runs: _containers.RepeatedCompositeFieldContainer[ReviewRun]
    def __init__(self, runs: _Optional[_Iterable[_Union[ReviewRun, _Mapping]]] = ...) -> None: ...

class ListReviewRunsRequest(_message.Message):
    __slots__ = ("change",)
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    change: str
    def __init__(self, change: _Optional[str] = ...) -> None: ...

class ListReviewRunsResponse(_message.Message):
    __slots__ = ("runs",)
    RUNS_FIELD_NUMBER: _ClassVar[int]
    runs: _containers.RepeatedCompositeFieldContainer[ReviewRun]
    def __init__(self, runs: _Optional[_Iterable[_Union[ReviewRun, _Mapping]]] = ...) -> None: ...

class ReviewRun(_message.Message):
    __slots__ = ("reviewer", "version", "reason", "status", "queued_at", "finished_at", "findings", "vote", "model", "detail", "kind")
    REVIEWER_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    QUEUED_AT_FIELD_NUMBER: _ClassVar[int]
    FINISHED_AT_FIELD_NUMBER: _ClassVar[int]
    FINDINGS_FIELD_NUMBER: _ClassVar[int]
    VOTE_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    DETAIL_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    reviewer: str
    version: str
    reason: str
    status: ReviewRunStatus
    queued_at: int
    finished_at: int
    findings: int
    vote: int
    model: str
    detail: str
    kind: str
    def __init__(self, reviewer: _Optional[str] = ..., version: _Optional[str] = ..., reason: _Optional[str] = ..., status: _Optional[_Union[ReviewRunStatus, str]] = ..., queued_at: _Optional[int] = ..., finished_at: _Optional[int] = ..., findings: _Optional[int] = ..., vote: _Optional[int] = ..., model: _Optional[str] = ..., detail: _Optional[str] = ..., kind: _Optional[str] = ...) -> None: ...

class LeaseResolveJobRequest(_message.Message):
    __slots__ = ("parent", "wait_ms")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    WAIT_MS_FIELD_NUMBER: _ClassVar[int]
    parent: str
    wait_ms: int
    def __init__(self, parent: _Optional[str] = ..., wait_ms: _Optional[int] = ...) -> None: ...

class LeaseResolveJobResponse(_message.Message):
    __slots__ = ("lease", "lease_until", "job")
    LEASE_FIELD_NUMBER: _ClassVar[int]
    LEASE_UNTIL_FIELD_NUMBER: _ClassVar[int]
    JOB_FIELD_NUMBER: _ClassVar[int]
    lease: bytes
    lease_until: int
    job: ResolveJob
    def __init__(self, lease: _Optional[bytes] = ..., lease_until: _Optional[int] = ..., job: _Optional[_Union[ResolveJob, _Mapping]] = ...) -> None: ...

class ResolveJob(_message.Message):
    __slots__ = ("change", "version", "paths", "prompt")
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    PATHS_FIELD_NUMBER: _ClassVar[int]
    PROMPT_FIELD_NUMBER: _ClassVar[int]
    change: str
    version: str
    paths: _containers.RepeatedScalarFieldContainer[str]
    prompt: str
    def __init__(self, change: _Optional[str] = ..., version: _Optional[str] = ..., paths: _Optional[_Iterable[str]] = ..., prompt: _Optional[str] = ...) -> None: ...

class CompleteResolveJobRequest(_message.Message):
    __slots__ = ("request_id", "parent", "lease", "resolved_version", "rationale")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    LEASE_FIELD_NUMBER: _ClassVar[int]
    RESOLVED_VERSION_FIELD_NUMBER: _ClassVar[int]
    RATIONALE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    lease: bytes
    resolved_version: str
    rationale: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., lease: _Optional[bytes] = ..., resolved_version: _Optional[str] = ..., rationale: _Optional[str] = ...) -> None: ...

class CompleteResolveJobResponse(_message.Message):
    __slots__ = ("run",)
    RUN_FIELD_NUMBER: _ClassVar[int]
    run: ReviewRun
    def __init__(self, run: _Optional[_Union[ReviewRun, _Mapping]] = ...) -> None: ...

class ReviewerStats(_message.Message):
    __slots__ = ("reviewer", "trust", "outcomes", "approvals", "escapes", "blocks", "dismissed", "demoted", "note", "since")
    REVIEWER_FIELD_NUMBER: _ClassVar[int]
    TRUST_FIELD_NUMBER: _ClassVar[int]
    OUTCOMES_FIELD_NUMBER: _ClassVar[int]
    APPROVALS_FIELD_NUMBER: _ClassVar[int]
    ESCAPES_FIELD_NUMBER: _ClassVar[int]
    BLOCKS_FIELD_NUMBER: _ClassVar[int]
    DISMISSED_FIELD_NUMBER: _ClassVar[int]
    DEMOTED_FIELD_NUMBER: _ClassVar[int]
    NOTE_FIELD_NUMBER: _ClassVar[int]
    SINCE_FIELD_NUMBER: _ClassVar[int]
    reviewer: str
    trust: int
    outcomes: int
    approvals: int
    escapes: int
    blocks: int
    dismissed: int
    demoted: bool
    note: str
    since: int
    def __init__(self, reviewer: _Optional[str] = ..., trust: _Optional[int] = ..., outcomes: _Optional[int] = ..., approvals: _Optional[int] = ..., escapes: _Optional[int] = ..., blocks: _Optional[int] = ..., dismissed: _Optional[int] = ..., demoted: _Optional[bool] = ..., note: _Optional[str] = ..., since: _Optional[int] = ...) -> None: ...

class ListReviewerStatsRequest(_message.Message):
    __slots__ = ("parent",)
    PARENT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    def __init__(self, parent: _Optional[str] = ...) -> None: ...

class ListReviewerStatsResponse(_message.Message):
    __slots__ = ("reviewers",)
    REVIEWERS_FIELD_NUMBER: _ClassVar[int]
    reviewers: _containers.RepeatedCompositeFieldContainer[ReviewerStats]
    def __init__(self, reviewers: _Optional[_Iterable[_Union[ReviewerStats, _Mapping]]] = ...) -> None: ...

class ReinstateReviewerRequest(_message.Message):
    __slots__ = ("request_id", "parent", "reviewer")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    REVIEWER_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    reviewer: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., reviewer: _Optional[str] = ...) -> None: ...

class ReinstateReviewerResponse(_message.Message):
    __slots__ = ("stats",)
    STATS_FIELD_NUMBER: _ClassVar[int]
    stats: ReviewerStats
    def __init__(self, stats: _Optional[_Union[ReviewerStats, _Mapping]] = ...) -> None: ...
