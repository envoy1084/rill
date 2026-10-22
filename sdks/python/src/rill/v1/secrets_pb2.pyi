from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class Secret(_message.Message):
    __slots__ = ("name", "repos", "trunk_only", "updated_at", "updated_by")
    NAME_FIELD_NUMBER: _ClassVar[int]
    REPOS_FIELD_NUMBER: _ClassVar[int]
    TRUNK_ONLY_FIELD_NUMBER: _ClassVar[int]
    UPDATED_AT_FIELD_NUMBER: _ClassVar[int]
    UPDATED_BY_FIELD_NUMBER: _ClassVar[int]
    name: str
    repos: _containers.RepeatedScalarFieldContainer[str]
    trunk_only: bool
    updated_at: int
    updated_by: _types_pb2.Principal
    def __init__(self, name: _Optional[str] = ..., repos: _Optional[_Iterable[str]] = ..., trunk_only: _Optional[bool] = ..., updated_at: _Optional[int] = ..., updated_by: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ...) -> None: ...

class UpdateSecretRequest(_message.Message):
    __slots__ = ("request_id", "secret", "value", "allow_missing")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    SECRET_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    ALLOW_MISSING_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    secret: Secret
    value: bytes
    allow_missing: bool
    def __init__(self, request_id: _Optional[str] = ..., secret: _Optional[_Union[Secret, _Mapping]] = ..., value: _Optional[bytes] = ..., allow_missing: _Optional[bool] = ...) -> None: ...

class UpdateSecretResponse(_message.Message):
    __slots__ = ("secret",)
    SECRET_FIELD_NUMBER: _ClassVar[int]
    secret: Secret
    def __init__(self, secret: _Optional[_Union[Secret, _Mapping]] = ...) -> None: ...

class DeleteSecretRequest(_message.Message):
    __slots__ = ("request_id", "name")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ...) -> None: ...

class DeleteSecretResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class ListSecretsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListSecretsResponse(_message.Message):
    __slots__ = ("secrets", "next_page_token")
    SECRETS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    secrets: _containers.RepeatedCompositeFieldContainer[Secret]
    next_page_token: str
    def __init__(self, secrets: _Optional[_Iterable[_Union[Secret, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...
