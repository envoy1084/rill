from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class LandStatus(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    LAND_STATUS_UNSPECIFIED: _ClassVar[LandStatus]
    LAND_STATUS_QUEUED: _ClassVar[LandStatus]
    LAND_STATUS_REBASING: _ClassVar[LandStatus]
    LAND_STATUS_LANDED: _ClassVar[LandStatus]
    LAND_STATUS_FAILED: _ClassVar[LandStatus]
    LAND_STATUS_CANCELLED: _ClassVar[LandStatus]
LAND_STATUS_UNSPECIFIED: LandStatus
LAND_STATUS_QUEUED: LandStatus
LAND_STATUS_REBASING: LandStatus
LAND_STATUS_LANDED: LandStatus
LAND_STATUS_FAILED: LandStatus
LAND_STATUS_CANCELLED: LandStatus

class LandRequest(_message.Message):
    __slots__ = ("name", "ticket", "change", "change_id", "status", "requested_at", "trunk_position", "landed_hash", "error", "queue_position", "eta_ms")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TICKET_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    REQUESTED_AT_FIELD_NUMBER: _ClassVar[int]
    TRUNK_POSITION_FIELD_NUMBER: _ClassVar[int]
    LANDED_HASH_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    QUEUE_POSITION_FIELD_NUMBER: _ClassVar[int]
    ETA_MS_FIELD_NUMBER: _ClassVar[int]
    name: str
    ticket: str
    change: str
    change_id: bytes
    status: LandStatus
    requested_at: int
    trunk_position: int
    landed_hash: bytes
    error: str
    queue_position: int
    eta_ms: int
    def __init__(self, name: _Optional[str] = ..., ticket: _Optional[str] = ..., change: _Optional[str] = ..., change_id: _Optional[bytes] = ..., status: _Optional[_Union[LandStatus, str]] = ..., requested_at: _Optional[int] = ..., trunk_position: _Optional[int] = ..., landed_hash: _Optional[bytes] = ..., error: _Optional[str] = ..., queue_position: _Optional[int] = ..., eta_ms: _Optional[int] = ...) -> None: ...

class RequestLandRequest(_message.Message):
    __slots__ = ("request_id", "change", "onto")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    ONTO_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    change: str
    onto: str
    def __init__(self, request_id: _Optional[str] = ..., change: _Optional[str] = ..., onto: _Optional[str] = ...) -> None: ...

class RequestLandResponse(_message.Message):
    __slots__ = ("land_request",)
    LAND_REQUEST_FIELD_NUMBER: _ClassVar[int]
    land_request: LandRequest
    def __init__(self, land_request: _Optional[_Union[LandRequest, _Mapping]] = ...) -> None: ...

class RequestLandStackRequest(_message.Message):
    __slots__ = ("request_id", "change", "prefix", "onto")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    PREFIX_FIELD_NUMBER: _ClassVar[int]
    ONTO_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    change: str
    prefix: bool
    onto: str
    def __init__(self, request_id: _Optional[str] = ..., change: _Optional[str] = ..., prefix: _Optional[bool] = ..., onto: _Optional[str] = ...) -> None: ...

class RequestLandStackResponse(_message.Message):
    __slots__ = ("land_requests",)
    LAND_REQUESTS_FIELD_NUMBER: _ClassVar[int]
    land_requests: _containers.RepeatedCompositeFieldContainer[LandRequest]
    def __init__(self, land_requests: _Optional[_Iterable[_Union[LandRequest, _Mapping]]] = ...) -> None: ...

class GetLandRequestRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetLandRequestResponse(_message.Message):
    __slots__ = ("land_request",)
    LAND_REQUEST_FIELD_NUMBER: _ClassVar[int]
    land_request: LandRequest
    def __init__(self, land_request: _Optional[_Union[LandRequest, _Mapping]] = ...) -> None: ...

class CancelLandRequestRequest(_message.Message):
    __slots__ = ("request_id", "name", "reason")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    reason: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class CancelLandRequestResponse(_message.Message):
    __slots__ = ("land_request",)
    LAND_REQUEST_FIELD_NUMBER: _ClassVar[int]
    land_request: LandRequest
    def __init__(self, land_request: _Optional[_Union[LandRequest, _Mapping]] = ...) -> None: ...

class ListLandRequestsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListLandRequestsResponse(_message.Message):
    __slots__ = ("land_requests", "next_page_token")
    LAND_REQUESTS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    land_requests: _containers.RepeatedCompositeFieldContainer[LandRequest]
    next_page_token: str
    def __init__(self, land_requests: _Optional[_Iterable[_Union[LandRequest, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...
