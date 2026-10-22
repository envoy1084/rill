from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class Relation(_message.Message):
    __slots__ = ("path", "relation", "subject")
    PATH_FIELD_NUMBER: _ClassVar[int]
    RELATION_FIELD_NUMBER: _ClassVar[int]
    SUBJECT_FIELD_NUMBER: _ClassVar[int]
    path: str
    relation: str
    subject: str
    def __init__(self, path: _Optional[str] = ..., relation: _Optional[str] = ..., subject: _Optional[str] = ...) -> None: ...

class CreateRelationRequest(_message.Message):
    __slots__ = ("request_id", "parent", "relation")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    RELATION_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    relation: Relation
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., relation: _Optional[_Union[Relation, _Mapping]] = ...) -> None: ...

class CreateRelationResponse(_message.Message):
    __slots__ = ("repo_uid", "relation_version")
    REPO_UID_FIELD_NUMBER: _ClassVar[int]
    RELATION_VERSION_FIELD_NUMBER: _ClassVar[int]
    repo_uid: str
    relation_version: int
    def __init__(self, repo_uid: _Optional[str] = ..., relation_version: _Optional[int] = ...) -> None: ...

class DeleteRelationRequest(_message.Message):
    __slots__ = ("request_id", "parent", "relation")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    RELATION_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    relation: Relation
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., relation: _Optional[_Union[Relation, _Mapping]] = ...) -> None: ...

class DeleteRelationResponse(_message.Message):
    __slots__ = ("repo_uid", "relation_version")
    REPO_UID_FIELD_NUMBER: _ClassVar[int]
    RELATION_VERSION_FIELD_NUMBER: _ClassVar[int]
    repo_uid: str
    relation_version: int
    def __init__(self, repo_uid: _Optional[str] = ..., relation_version: _Optional[int] = ...) -> None: ...

class ListRelationsRequest(_message.Message):
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

class ListRelationsResponse(_message.Message):
    __slots__ = ("relations", "relation_version", "next_page_token")
    RELATIONS_FIELD_NUMBER: _ClassVar[int]
    RELATION_VERSION_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    relations: _containers.RepeatedCompositeFieldContainer[Relation]
    relation_version: int
    next_page_token: str
    def __init__(self, relations: _Optional[_Iterable[_Union[Relation, _Mapping]]] = ..., relation_version: _Optional[int] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class CheckAccessRequest(_message.Message):
    __slots__ = ("parent", "path", "relation", "subject")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    RELATION_FIELD_NUMBER: _ClassVar[int]
    SUBJECT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    path: str
    relation: str
    subject: str
    def __init__(self, parent: _Optional[str] = ..., path: _Optional[str] = ..., relation: _Optional[str] = ..., subject: _Optional[str] = ...) -> None: ...

class CheckAccessResponse(_message.Message):
    __slots__ = ("allowed", "because", "org_admin", "relation_version")
    ALLOWED_FIELD_NUMBER: _ClassVar[int]
    BECAUSE_FIELD_NUMBER: _ClassVar[int]
    ORG_ADMIN_FIELD_NUMBER: _ClassVar[int]
    RELATION_VERSION_FIELD_NUMBER: _ClassVar[int]
    allowed: bool
    because: _containers.RepeatedCompositeFieldContainer[Relation]
    org_admin: bool
    relation_version: int
    def __init__(self, allowed: _Optional[bool] = ..., because: _Optional[_Iterable[_Union[Relation, _Mapping]]] = ..., org_admin: _Optional[bool] = ..., relation_version: _Optional[int] = ...) -> None: ...

class ExpandAccessRequest(_message.Message):
    __slots__ = ("parent", "path", "relation")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    RELATION_FIELD_NUMBER: _ClassVar[int]
    parent: str
    path: str
    relation: str
    def __init__(self, parent: _Optional[str] = ..., path: _Optional[str] = ..., relation: _Optional[str] = ...) -> None: ...

class ExpandedTuple(_message.Message):
    __slots__ = ("tuple", "members")
    TUPLE_FIELD_NUMBER: _ClassVar[int]
    MEMBERS_FIELD_NUMBER: _ClassVar[int]
    tuple: Relation
    members: _containers.RepeatedCompositeFieldContainer[_types_pb2.Principal]
    def __init__(self, tuple: _Optional[_Union[Relation, _Mapping]] = ..., members: _Optional[_Iterable[_Union[_types_pb2.Principal, _Mapping]]] = ...) -> None: ...

class ExpandAccessResponse(_message.Message):
    __slots__ = ("tuples", "relation_version")
    TUPLES_FIELD_NUMBER: _ClassVar[int]
    RELATION_VERSION_FIELD_NUMBER: _ClassVar[int]
    tuples: _containers.RepeatedCompositeFieldContainer[ExpandedTuple]
    relation_version: int
    def __init__(self, tuples: _Optional[_Iterable[_Union[ExpandedTuple, _Mapping]]] = ..., relation_version: _Optional[int] = ...) -> None: ...

class LookupPathsRequest(_message.Message):
    __slots__ = ("parent", "relation", "subject")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    RELATION_FIELD_NUMBER: _ClassVar[int]
    SUBJECT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    relation: str
    subject: str
    def __init__(self, parent: _Optional[str] = ..., relation: _Optional[str] = ..., subject: _Optional[str] = ...) -> None: ...

class LookupPathsResponse(_message.Message):
    __slots__ = ("paths", "relation_version")
    PATHS_FIELD_NUMBER: _ClassVar[int]
    RELATION_VERSION_FIELD_NUMBER: _ClassVar[int]
    paths: _containers.RepeatedScalarFieldContainer[str]
    relation_version: int
    def __init__(self, paths: _Optional[_Iterable[str]] = ..., relation_version: _Optional[int] = ...) -> None: ...
