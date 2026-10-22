from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class Redaction(_message.Message):
    __slots__ = ("name", "id", "org", "kind", "hash", "reason", "requested_by", "requested_at", "approved_by", "applied_at")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ID_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    HASH_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    REQUESTED_BY_FIELD_NUMBER: _ClassVar[int]
    REQUESTED_AT_FIELD_NUMBER: _ClassVar[int]
    APPROVED_BY_FIELD_NUMBER: _ClassVar[int]
    APPLIED_AT_FIELD_NUMBER: _ClassVar[int]
    name: str
    id: str
    org: str
    kind: _types_pb2.ObjectKind
    hash: bytes
    reason: str
    requested_by: _types_pb2.Principal
    requested_at: int
    approved_by: _types_pb2.Principal
    applied_at: int
    def __init__(self, name: _Optional[str] = ..., id: _Optional[str] = ..., org: _Optional[str] = ..., kind: _Optional[_Union[_types_pb2.ObjectKind, str]] = ..., hash: _Optional[bytes] = ..., reason: _Optional[str] = ..., requested_by: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., requested_at: _Optional[int] = ..., approved_by: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., applied_at: _Optional[int] = ...) -> None: ...

class CreateRedactionRequest(_message.Message):
    __slots__ = ("request_id", "parent", "kind", "hash", "reason")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    HASH_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    kind: _types_pb2.ObjectKind
    hash: bytes
    reason: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., kind: _Optional[_Union[_types_pb2.ObjectKind, str]] = ..., hash: _Optional[bytes] = ..., reason: _Optional[str] = ...) -> None: ...

class CreateRedactionResponse(_message.Message):
    __slots__ = ("redaction",)
    REDACTION_FIELD_NUMBER: _ClassVar[int]
    redaction: Redaction
    def __init__(self, redaction: _Optional[_Union[Redaction, _Mapping]] = ...) -> None: ...

class ApproveRedactionRequest(_message.Message):
    __slots__ = ("request_id", "name")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ...) -> None: ...

class ApproveRedactionResponse(_message.Message):
    __slots__ = ("redaction",)
    REDACTION_FIELD_NUMBER: _ClassVar[int]
    redaction: Redaction
    def __init__(self, redaction: _Optional[_Union[Redaction, _Mapping]] = ...) -> None: ...

class ListRedactionsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListRedactionsResponse(_message.Message):
    __slots__ = ("redactions", "next_page_token")
    REDACTIONS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    redactions: _containers.RepeatedCompositeFieldContainer[Redaction]
    next_page_token: str
    def __init__(self, redactions: _Optional[_Iterable[_Union[Redaction, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class ReleaseVersionRequest(_message.Message):
    __slots__ = ("request_id", "version")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    version: str
    def __init__(self, request_id: _Optional[str] = ..., version: _Optional[str] = ...) -> None: ...

class ReleaseVersionResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class RotateDataKeysRequest(_message.Message):
    __slots__ = ("request_id", "parent", "rewrap")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    REWRAP_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    rewrap: bool
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., rewrap: _Optional[bool] = ...) -> None: ...

class RotateDataKeysResponse(_message.Message):
    __slots__ = ("key_id", "active_from", "rewrapped")
    KEY_ID_FIELD_NUMBER: _ClassVar[int]
    ACTIVE_FROM_FIELD_NUMBER: _ClassVar[int]
    REWRAPPED_FIELD_NUMBER: _ClassVar[int]
    key_id: int
    active_from: int
    rewrapped: int
    def __init__(self, key_id: _Optional[int] = ..., active_from: _Optional[int] = ..., rewrapped: _Optional[int] = ...) -> None: ...
