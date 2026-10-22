from rill.v1 import landing_pb2 as _landing_pb2
from rill.v1 import operations_pb2 as _operations_pb2
from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class VerificationStatus(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    VERIFICATION_STATUS_UNSPECIFIED: _ClassVar[VerificationStatus]
    VERIFICATION_STATUS_PENDING: _ClassVar[VerificationStatus]
    VERIFICATION_STATUS_RUNNING: _ClassVar[VerificationStatus]
    VERIFICATION_STATUS_PASSED: _ClassVar[VerificationStatus]
    VERIFICATION_STATUS_FAILED: _ClassVar[VerificationStatus]
    VERIFICATION_STATUS_INHERITED: _ClassVar[VerificationStatus]
    VERIFICATION_STATUS_SKIPPED: _ClassVar[VerificationStatus]
VERIFICATION_STATUS_UNSPECIFIED: VerificationStatus
VERIFICATION_STATUS_PENDING: VerificationStatus
VERIFICATION_STATUS_RUNNING: VerificationStatus
VERIFICATION_STATUS_PASSED: VerificationStatus
VERIFICATION_STATUS_FAILED: VerificationStatus
VERIFICATION_STATUS_INHERITED: VerificationStatus
VERIFICATION_STATUS_SKIPPED: VerificationStatus

class VerificationReport(_message.Message):
    __slots__ = ("target", "status", "reporter", "reported_at", "details", "artifact")
    TARGET_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    REPORTER_FIELD_NUMBER: _ClassVar[int]
    REPORTED_AT_FIELD_NUMBER: _ClassVar[int]
    DETAILS_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_FIELD_NUMBER: _ClassVar[int]
    target: str
    status: VerificationStatus
    reporter: _types_pb2.Principal
    reported_at: int
    details: str
    artifact: bytes
    def __init__(self, target: _Optional[str] = ..., status: _Optional[_Union[VerificationStatus, str]] = ..., reporter: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., reported_at: _Optional[int] = ..., details: _Optional[str] = ..., artifact: _Optional[bytes] = ...) -> None: ...

class Verification(_message.Message):
    __slots__ = ("landing", "position", "status", "culprit", "reports")
    LANDING_FIELD_NUMBER: _ClassVar[int]
    POSITION_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    CULPRIT_FIELD_NUMBER: _ClassVar[int]
    REPORTS_FIELD_NUMBER: _ClassVar[int]
    landing: str
    position: int
    status: VerificationStatus
    culprit: int
    reports: _containers.RepeatedCompositeFieldContainer[VerificationReport]
    def __init__(self, landing: _Optional[str] = ..., position: _Optional[int] = ..., status: _Optional[_Union[VerificationStatus, str]] = ..., culprit: _Optional[int] = ..., reports: _Optional[_Iterable[_Union[VerificationReport, _Mapping]]] = ...) -> None: ...

class GetTrunkStatusRequest(_message.Message):
    __slots__ = ("parent",)
    PARENT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    def __init__(self, parent: _Optional[str] = ...) -> None: ...

class GetTrunkStatusResponse(_message.Message):
    __slots__ = ("head_position", "head_hash", "in_flight", "last_verified_position")
    HEAD_POSITION_FIELD_NUMBER: _ClassVar[int]
    HEAD_HASH_FIELD_NUMBER: _ClassVar[int]
    IN_FLIGHT_FIELD_NUMBER: _ClassVar[int]
    LAST_VERIFIED_POSITION_FIELD_NUMBER: _ClassVar[int]
    head_position: int
    head_hash: bytes
    in_flight: _containers.RepeatedCompositeFieldContainer[_landing_pb2.LandRequest]
    last_verified_position: int
    def __init__(self, head_position: _Optional[int] = ..., head_hash: _Optional[bytes] = ..., in_flight: _Optional[_Iterable[_Union[_landing_pb2.LandRequest, _Mapping]]] = ..., last_verified_position: _Optional[int] = ...) -> None: ...

class ListLandingsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListLandingsResponse(_message.Message):
    __slots__ = ("landings", "verifications", "next_page_token")
    LANDINGS_FIELD_NUMBER: _ClassVar[int]
    VERIFICATIONS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    landings: _containers.RepeatedCompositeFieldContainer[_types_pb2.Landing]
    verifications: _containers.RepeatedCompositeFieldContainer[Verification]
    next_page_token: str
    def __init__(self, landings: _Optional[_Iterable[_Union[_types_pb2.Landing, _Mapping]]] = ..., verifications: _Optional[_Iterable[_Union[Verification, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class GetVerificationRequest(_message.Message):
    __slots__ = ("landing",)
    LANDING_FIELD_NUMBER: _ClassVar[int]
    landing: str
    def __init__(self, landing: _Optional[str] = ...) -> None: ...

class GetVerificationResponse(_message.Message):
    __slots__ = ("verification",)
    VERIFICATION_FIELD_NUMBER: _ClassVar[int]
    verification: Verification
    def __init__(self, verification: _Optional[_Union[Verification, _Mapping]] = ...) -> None: ...

class ReportVerificationRequest(_message.Message):
    __slots__ = ("request_id", "landing", "target", "status", "details", "artifact")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    LANDING_FIELD_NUMBER: _ClassVar[int]
    TARGET_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    DETAILS_FIELD_NUMBER: _ClassVar[int]
    ARTIFACT_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    landing: str
    target: str
    status: VerificationStatus
    details: str
    artifact: bytes
    def __init__(self, request_id: _Optional[str] = ..., landing: _Optional[str] = ..., target: _Optional[str] = ..., status: _Optional[_Union[VerificationStatus, str]] = ..., details: _Optional[str] = ..., artifact: _Optional[bytes] = ...) -> None: ...

class ReportVerificationResponse(_message.Message):
    __slots__ = ("verification",)
    VERIFICATION_FIELD_NUMBER: _ClassVar[int]
    verification: Verification
    def __init__(self, verification: _Optional[_Union[Verification, _Mapping]] = ...) -> None: ...

class RevertLandingRequest(_message.Message):
    __slots__ = ("request_id", "landing", "reason")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    LANDING_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    landing: str
    reason: str
    def __init__(self, request_id: _Optional[str] = ..., landing: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class RevertLandingResponse(_message.Message):
    __slots__ = ("landing", "version")
    LANDING_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    landing: _types_pb2.Landing
    version: _types_pb2.ChangeVersion
    def __init__(self, landing: _Optional[_Union[_types_pb2.Landing, _Mapping]] = ..., version: _Optional[_Union[_types_pb2.ChangeVersion, _Mapping]] = ...) -> None: ...

class RevertSelector(_message.Message):
    __slots__ = ("session", "model", "sponsor", "author", "intent", "since", "until")
    SESSION_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    SPONSOR_FIELD_NUMBER: _ClassVar[int]
    AUTHOR_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    SINCE_FIELD_NUMBER: _ClassVar[int]
    UNTIL_FIELD_NUMBER: _ClassVar[int]
    session: str
    model: str
    sponsor: str
    author: str
    intent: str
    since: int
    until: int
    def __init__(self, session: _Optional[str] = ..., model: _Optional[str] = ..., sponsor: _Optional[str] = ..., author: _Optional[str] = ..., intent: _Optional[str] = ..., since: _Optional[int] = ..., until: _Optional[int] = ...) -> None: ...

class PreviewBulkRevertRequest(_message.Message):
    __slots__ = ("parent", "selector")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    SELECTOR_FIELD_NUMBER: _ClassVar[int]
    parent: str
    selector: RevertSelector
    def __init__(self, parent: _Optional[str] = ..., selector: _Optional[_Union[RevertSelector, _Mapping]] = ...) -> None: ...

class PreviewBulkRevertResponse(_message.Message):
    __slots__ = ("entries",)
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    entries: _containers.RepeatedCompositeFieldContainer[_operations_pb2.BulkRevertEntry]
    def __init__(self, entries: _Optional[_Iterable[_Union[_operations_pb2.BulkRevertEntry, _Mapping]]] = ...) -> None: ...

class BulkRevertRequest(_message.Message):
    __slots__ = ("request_id", "parent", "selector", "reason")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    SELECTOR_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    selector: RevertSelector
    reason: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., selector: _Optional[_Union[RevertSelector, _Mapping]] = ..., reason: _Optional[str] = ...) -> None: ...

class BulkRevertResponse(_message.Message):
    __slots__ = ("operation",)
    OPERATION_FIELD_NUMBER: _ClassVar[int]
    operation: _operations_pb2.Operation
    def __init__(self, operation: _Optional[_Union[_operations_pb2.Operation, _Mapping]] = ...) -> None: ...

class BisectRequest(_message.Message):
    __slots__ = ("request_id", "parent", "to", "target")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    FROM_FIELD_NUMBER: _ClassVar[int]
    TO_FIELD_NUMBER: _ClassVar[int]
    TARGET_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    to: int
    target: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., to: _Optional[int] = ..., target: _Optional[str] = ..., **kwargs) -> None: ...

class BisectResponse(_message.Message):
    __slots__ = ("culprit", "tested")
    CULPRIT_FIELD_NUMBER: _ClassVar[int]
    TESTED_FIELD_NUMBER: _ClassVar[int]
    culprit: int
    tested: _containers.RepeatedCompositeFieldContainer[Verification]
    def __init__(self, culprit: _Optional[int] = ..., tested: _Optional[_Iterable[_Union[Verification, _Mapping]]] = ...) -> None: ...

class TrunkHealth(_message.Message):
    __slots__ = ("state", "since", "open", "failing_steps", "frozen", "frozen_reason")
    STATE_FIELD_NUMBER: _ClassVar[int]
    SINCE_FIELD_NUMBER: _ClassVar[int]
    OPEN_FIELD_NUMBER: _ClassVar[int]
    FAILING_STEPS_FIELD_NUMBER: _ClassVar[int]
    FROZEN_FIELD_NUMBER: _ClassVar[int]
    FROZEN_REASON_FIELD_NUMBER: _ClassVar[int]
    state: str
    since: int
    open: _containers.RepeatedScalarFieldContainer[int]
    failing_steps: _containers.RepeatedScalarFieldContainer[str]
    frozen: bool
    frozen_reason: str
    def __init__(self, state: _Optional[str] = ..., since: _Optional[int] = ..., open: _Optional[_Iterable[int]] = ..., failing_steps: _Optional[_Iterable[str]] = ..., frozen: _Optional[bool] = ..., frozen_reason: _Optional[str] = ...) -> None: ...

class Incident(_message.Message):
    __slots__ = ("name", "culprit", "version", "change_id", "failures", "steps", "details", "author", "session", "model", "sponsor", "state", "opened_at", "updated_at", "fix", "resolution", "resolved_at", "revert_blocked", "assigned")
    NAME_FIELD_NUMBER: _ClassVar[int]
    CULPRIT_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    FAILURES_FIELD_NUMBER: _ClassVar[int]
    STEPS_FIELD_NUMBER: _ClassVar[int]
    DETAILS_FIELD_NUMBER: _ClassVar[int]
    AUTHOR_FIELD_NUMBER: _ClassVar[int]
    SESSION_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    SPONSOR_FIELD_NUMBER: _ClassVar[int]
    STATE_FIELD_NUMBER: _ClassVar[int]
    OPENED_AT_FIELD_NUMBER: _ClassVar[int]
    UPDATED_AT_FIELD_NUMBER: _ClassVar[int]
    FIX_FIELD_NUMBER: _ClassVar[int]
    RESOLUTION_FIELD_NUMBER: _ClassVar[int]
    RESOLVED_AT_FIELD_NUMBER: _ClassVar[int]
    REVERT_BLOCKED_FIELD_NUMBER: _ClassVar[int]
    ASSIGNED_FIELD_NUMBER: _ClassVar[int]
    name: str
    culprit: int
    version: bytes
    change_id: bytes
    failures: _containers.RepeatedScalarFieldContainer[str]
    steps: _containers.RepeatedScalarFieldContainer[str]
    details: str
    author: _types_pb2.Principal
    session: str
    model: str
    sponsor: _types_pb2.Principal
    state: str
    opened_at: int
    updated_at: int
    fix: int
    resolution: str
    resolved_at: int
    revert_blocked: str
    assigned: str
    def __init__(self, name: _Optional[str] = ..., culprit: _Optional[int] = ..., version: _Optional[bytes] = ..., change_id: _Optional[bytes] = ..., failures: _Optional[_Iterable[str]] = ..., steps: _Optional[_Iterable[str]] = ..., details: _Optional[str] = ..., author: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., session: _Optional[str] = ..., model: _Optional[str] = ..., sponsor: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., state: _Optional[str] = ..., opened_at: _Optional[int] = ..., updated_at: _Optional[int] = ..., fix: _Optional[int] = ..., resolution: _Optional[str] = ..., resolved_at: _Optional[int] = ..., revert_blocked: _Optional[str] = ..., assigned: _Optional[str] = ...) -> None: ...

class GetTrunkHealthRequest(_message.Message):
    __slots__ = ("parent",)
    PARENT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    def __init__(self, parent: _Optional[str] = ...) -> None: ...

class GetTrunkHealthResponse(_message.Message):
    __slots__ = ("health", "open")
    HEALTH_FIELD_NUMBER: _ClassVar[int]
    OPEN_FIELD_NUMBER: _ClassVar[int]
    health: TrunkHealth
    open: _containers.RepeatedCompositeFieldContainer[Incident]
    def __init__(self, health: _Optional[_Union[TrunkHealth, _Mapping]] = ..., open: _Optional[_Iterable[_Union[Incident, _Mapping]]] = ...) -> None: ...

class ListIncidentsRequest(_message.Message):
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

class ListIncidentsResponse(_message.Message):
    __slots__ = ("incidents", "next_page_token")
    INCIDENTS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    incidents: _containers.RepeatedCompositeFieldContainer[Incident]
    next_page_token: str
    def __init__(self, incidents: _Optional[_Iterable[_Union[Incident, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class FreezeTrunkRequest(_message.Message):
    __slots__ = ("request_id", "parent", "reason")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    reason: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class FreezeTrunkResponse(_message.Message):
    __slots__ = ("health",)
    HEALTH_FIELD_NUMBER: _ClassVar[int]
    health: TrunkHealth
    def __init__(self, health: _Optional[_Union[TrunkHealth, _Mapping]] = ...) -> None: ...

class UnfreezeTrunkRequest(_message.Message):
    __slots__ = ("request_id", "parent")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ...) -> None: ...

class UnfreezeTrunkResponse(_message.Message):
    __slots__ = ("health",)
    HEALTH_FIELD_NUMBER: _ClassVar[int]
    health: TrunkHealth
    def __init__(self, health: _Optional[_Union[TrunkHealth, _Mapping]] = ...) -> None: ...
