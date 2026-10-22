from google.rpc import status_pb2 as _status_pb2
from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class Operation(_message.Message):
    __slots__ = ("name", "kind", "done", "creator", "created_at", "updated_at", "error", "bulk_revert")
    NAME_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    DONE_FIELD_NUMBER: _ClassVar[int]
    CREATOR_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    UPDATED_AT_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    BULK_REVERT_FIELD_NUMBER: _ClassVar[int]
    name: str
    kind: str
    done: bool
    creator: _types_pb2.Principal
    created_at: int
    updated_at: int
    error: _status_pb2.Status
    bulk_revert: BulkRevertResult
    def __init__(self, name: _Optional[str] = ..., kind: _Optional[str] = ..., done: _Optional[bool] = ..., creator: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., created_at: _Optional[int] = ..., updated_at: _Optional[int] = ..., error: _Optional[_Union[_status_pb2.Status, _Mapping]] = ..., bulk_revert: _Optional[_Union[BulkRevertResult, _Mapping]] = ...) -> None: ...

class BulkRevertEntry(_message.Message):
    __slots__ = ("landing", "position", "change_id", "summary", "conflicts", "land_request")
    LANDING_FIELD_NUMBER: _ClassVar[int]
    POSITION_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    SUMMARY_FIELD_NUMBER: _ClassVar[int]
    CONFLICTS_FIELD_NUMBER: _ClassVar[int]
    LAND_REQUEST_FIELD_NUMBER: _ClassVar[int]
    landing: str
    position: int
    change_id: bytes
    summary: str
    conflicts: _containers.RepeatedScalarFieldContainer[str]
    land_request: str
    def __init__(self, landing: _Optional[str] = ..., position: _Optional[int] = ..., change_id: _Optional[bytes] = ..., summary: _Optional[str] = ..., conflicts: _Optional[_Iterable[str]] = ..., land_request: _Optional[str] = ...) -> None: ...

class BulkRevertResult(_message.Message):
    __slots__ = ("entries",)
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    entries: _containers.RepeatedCompositeFieldContainer[BulkRevertEntry]
    def __init__(self, entries: _Optional[_Iterable[_Union[BulkRevertEntry, _Mapping]]] = ...) -> None: ...

class GetOperationRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetOperationResponse(_message.Message):
    __slots__ = ("operation",)
    OPERATION_FIELD_NUMBER: _ClassVar[int]
    operation: Operation
    def __init__(self, operation: _Optional[_Union[Operation, _Mapping]] = ...) -> None: ...

class ListOperationsRequest(_message.Message):
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

class ListOperationsResponse(_message.Message):
    __slots__ = ("operations", "next_page_token")
    OPERATIONS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    operations: _containers.RepeatedCompositeFieldContainer[Operation]
    next_page_token: str
    def __init__(self, operations: _Optional[_Iterable[_Union[Operation, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class WaitOperationRequest(_message.Message):
    __slots__ = ("name", "timeout_ms")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    name: str
    timeout_ms: int
    def __init__(self, name: _Optional[str] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class WaitOperationResponse(_message.Message):
    __slots__ = ("operation",)
    OPERATION_FIELD_NUMBER: _ClassVar[int]
    operation: Operation
    def __init__(self, operation: _Optional[_Union[Operation, _Mapping]] = ...) -> None: ...
