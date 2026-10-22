from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class CreateOrgRequest(_message.Message):
    __slots__ = ("request_id", "org_id", "description")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    ORG_ID_FIELD_NUMBER: _ClassVar[int]
    DESCRIPTION_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    org_id: str
    description: str
    def __init__(self, request_id: _Optional[str] = ..., org_id: _Optional[str] = ..., description: _Optional[str] = ...) -> None: ...

class CreateOrgResponse(_message.Message):
    __slots__ = ("org",)
    ORG_FIELD_NUMBER: _ClassVar[int]
    org: _types_pb2.Org
    def __init__(self, org: _Optional[_Union[_types_pb2.Org, _Mapping]] = ...) -> None: ...

class GetOrgRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetOrgResponse(_message.Message):
    __slots__ = ("org",)
    ORG_FIELD_NUMBER: _ClassVar[int]
    org: _types_pb2.Org
    def __init__(self, org: _Optional[_Union[_types_pb2.Org, _Mapping]] = ...) -> None: ...

class ListOrgsRequest(_message.Message):
    __slots__ = ("page_size", "page_token")
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    page_size: int
    page_token: str
    def __init__(self, page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListOrgsResponse(_message.Message):
    __slots__ = ("orgs", "next_page_token")
    ORGS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    orgs: _containers.RepeatedCompositeFieldContainer[_types_pb2.Org]
    next_page_token: str
    def __init__(self, orgs: _Optional[_Iterable[_Union[_types_pb2.Org, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...
