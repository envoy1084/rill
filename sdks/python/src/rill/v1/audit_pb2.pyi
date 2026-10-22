from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class AuditEntryRecord(_message.Message):
    __slots__ = ("seq", "org", "at", "actor", "action", "target", "allowed", "denied_reason", "detail", "hash", "prev", "encoded")
    class DetailEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    SEQ_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    AT_FIELD_NUMBER: _ClassVar[int]
    ACTOR_FIELD_NUMBER: _ClassVar[int]
    ACTION_FIELD_NUMBER: _ClassVar[int]
    TARGET_FIELD_NUMBER: _ClassVar[int]
    ALLOWED_FIELD_NUMBER: _ClassVar[int]
    DENIED_REASON_FIELD_NUMBER: _ClassVar[int]
    DETAIL_FIELD_NUMBER: _ClassVar[int]
    HASH_FIELD_NUMBER: _ClassVar[int]
    PREV_FIELD_NUMBER: _ClassVar[int]
    ENCODED_FIELD_NUMBER: _ClassVar[int]
    seq: int
    org: str
    at: int
    actor: _types_pb2.Principal
    action: str
    target: str
    allowed: bool
    denied_reason: str
    detail: _containers.ScalarMap[str, str]
    hash: bytes
    prev: bytes
    encoded: bytes
    def __init__(self, seq: _Optional[int] = ..., org: _Optional[str] = ..., at: _Optional[int] = ..., actor: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., action: _Optional[str] = ..., target: _Optional[str] = ..., allowed: _Optional[bool] = ..., denied_reason: _Optional[str] = ..., detail: _Optional[_Mapping[str, str]] = ..., hash: _Optional[bytes] = ..., prev: _Optional[bytes] = ..., encoded: _Optional[bytes] = ...) -> None: ...

class AuditRootRecord(_message.Message):
    __slots__ = ("first_seq", "last_seq", "head", "export", "sealed_at", "key_id", "encoded")
    FIRST_SEQ_FIELD_NUMBER: _ClassVar[int]
    LAST_SEQ_FIELD_NUMBER: _ClassVar[int]
    HEAD_FIELD_NUMBER: _ClassVar[int]
    EXPORT_FIELD_NUMBER: _ClassVar[int]
    SEALED_AT_FIELD_NUMBER: _ClassVar[int]
    KEY_ID_FIELD_NUMBER: _ClassVar[int]
    ENCODED_FIELD_NUMBER: _ClassVar[int]
    first_seq: int
    last_seq: int
    head: bytes
    export: bytes
    sealed_at: int
    key_id: int
    encoded: bytes
    def __init__(self, first_seq: _Optional[int] = ..., last_seq: _Optional[int] = ..., head: _Optional[bytes] = ..., export: _Optional[bytes] = ..., sealed_at: _Optional[int] = ..., key_id: _Optional[int] = ..., encoded: _Optional[bytes] = ...) -> None: ...

class ListAuditEntriesRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token", "after_seq")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    AFTER_SEQ_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    after_seq: int
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ..., after_seq: _Optional[int] = ...) -> None: ...

class ListAuditEntriesResponse(_message.Message):
    __slots__ = ("entries", "head_seq", "next_page_token")
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    HEAD_SEQ_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    entries: _containers.RepeatedCompositeFieldContainer[AuditEntryRecord]
    head_seq: int
    next_page_token: str
    def __init__(self, entries: _Optional[_Iterable[_Union[AuditEntryRecord, _Mapping]]] = ..., head_seq: _Optional[int] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class ListAuditRootsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListAuditRootsResponse(_message.Message):
    __slots__ = ("roots", "next_page_token")
    ROOTS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    roots: _containers.RepeatedCompositeFieldContainer[AuditRootRecord]
    next_page_token: str
    def __init__(self, roots: _Optional[_Iterable[_Union[AuditRootRecord, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class SealAuditRequest(_message.Message):
    __slots__ = ("parent",)
    PARENT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    def __init__(self, parent: _Optional[str] = ...) -> None: ...

class SealAuditResponse(_message.Message):
    __slots__ = ("root",)
    ROOT_FIELD_NUMBER: _ClassVar[int]
    root: AuditRootRecord
    def __init__(self, root: _Optional[_Union[AuditRootRecord, _Mapping]] = ...) -> None: ...

class WatchAuditRequest(_message.Message):
    __slots__ = ("parent", "after_seq")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    AFTER_SEQ_FIELD_NUMBER: _ClassVar[int]
    parent: str
    after_seq: int
    def __init__(self, parent: _Optional[str] = ..., after_seq: _Optional[int] = ...) -> None: ...

class WatchAuditResponse(_message.Message):
    __slots__ = ("entry",)
    ENTRY_FIELD_NUMBER: _ClassVar[int]
    entry: AuditEntryRecord
    def __init__(self, entry: _Optional[_Union[AuditEntryRecord, _Mapping]] = ...) -> None: ...

class ListAuditKeysRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class ListAuditKeysResponse(_message.Message):
    __slots__ = ("keys",)
    KEYS_FIELD_NUMBER: _ClassVar[int]
    keys: _containers.RepeatedCompositeFieldContainer[AuditKey]
    def __init__(self, keys: _Optional[_Iterable[_Union[AuditKey, _Mapping]]] = ...) -> None: ...

class AuditKey(_message.Message):
    __slots__ = ("id", "public_key")
    ID_FIELD_NUMBER: _ClassVar[int]
    PUBLIC_KEY_FIELD_NUMBER: _ClassVar[int]
    id: int
    public_key: bytes
    def __init__(self, id: _Optional[int] = ..., public_key: _Optional[bytes] = ...) -> None: ...
