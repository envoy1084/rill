from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class GetRepoRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetRepoResponse(_message.Message):
    __slots__ = ("repo",)
    REPO_FIELD_NUMBER: _ClassVar[int]
    repo: _types_pb2.Repo
    def __init__(self, repo: _Optional[_Union[_types_pb2.Repo, _Mapping]] = ...) -> None: ...

class ListReposRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListReposResponse(_message.Message):
    __slots__ = ("repos", "next_page_token")
    REPOS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    repos: _containers.RepeatedCompositeFieldContainer[_types_pb2.Repo]
    next_page_token: str
    def __init__(self, repos: _Optional[_Iterable[_Union[_types_pb2.Repo, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class CreateRepoRequest(_message.Message):
    __slots__ = ("request_id", "parent", "repo_id", "repo")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    REPO_ID_FIELD_NUMBER: _ClassVar[int]
    REPO_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    repo_id: str
    repo: _types_pb2.Repo
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., repo_id: _Optional[str] = ..., repo: _Optional[_Union[_types_pb2.Repo, _Mapping]] = ...) -> None: ...

class CreateRepoResponse(_message.Message):
    __slots__ = ("repo",)
    REPO_FIELD_NUMBER: _ClassVar[int]
    repo: _types_pb2.Repo
    def __init__(self, repo: _Optional[_Union[_types_pb2.Repo, _Mapping]] = ...) -> None: ...

class ResolveRevisionRequest(_message.Message):
    __slots__ = ("repo", "revision")
    REPO_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    repo: str
    revision: str
    def __init__(self, repo: _Optional[str] = ..., revision: _Optional[str] = ...) -> None: ...

class ResolveRevisionResponse(_message.Message):
    __slots__ = ("version", "version_hash", "tree_hash", "trunk_position")
    VERSION_FIELD_NUMBER: _ClassVar[int]
    VERSION_HASH_FIELD_NUMBER: _ClassVar[int]
    TREE_HASH_FIELD_NUMBER: _ClassVar[int]
    TRUNK_POSITION_FIELD_NUMBER: _ClassVar[int]
    version: str
    version_hash: bytes
    tree_hash: bytes
    trunk_position: int
    def __init__(self, version: _Optional[str] = ..., version_hash: _Optional[bytes] = ..., tree_hash: _Optional[bytes] = ..., trunk_position: _Optional[int] = ...) -> None: ...

class GetRefRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetRefResponse(_message.Message):
    __slots__ = ("ref",)
    REF_FIELD_NUMBER: _ClassVar[int]
    ref: _types_pb2.Ref
    def __init__(self, ref: _Optional[_Union[_types_pb2.Ref, _Mapping]] = ...) -> None: ...

class ListRefsRequest(_message.Message):
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

class ListRefsResponse(_message.Message):
    __slots__ = ("refs", "next_page_token")
    REFS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    refs: _containers.RepeatedCompositeFieldContainer[_types_pb2.Ref]
    next_page_token: str
    def __init__(self, refs: _Optional[_Iterable[_Union[_types_pb2.Ref, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class UpdateRefRequest(_message.Message):
    __slots__ = ("request_id", "ref", "etag", "allow_missing")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    REF_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    ALLOW_MISSING_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    ref: _types_pb2.Ref
    etag: str
    allow_missing: bool
    def __init__(self, request_id: _Optional[str] = ..., ref: _Optional[_Union[_types_pb2.Ref, _Mapping]] = ..., etag: _Optional[str] = ..., allow_missing: _Optional[bool] = ...) -> None: ...

class UpdateRefResponse(_message.Message):
    __slots__ = ("ref",)
    REF_FIELD_NUMBER: _ClassVar[int]
    ref: _types_pb2.Ref
    def __init__(self, ref: _Optional[_Union[_types_pb2.Ref, _Mapping]] = ...) -> None: ...

class DeleteRefRequest(_message.Message):
    __slots__ = ("request_id", "name", "etag")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    etag: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., etag: _Optional[str] = ...) -> None: ...

class DeleteRefResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class CreateReleaseRequest(_message.Message):
    __slots__ = ("request_id", "parent", "release_id", "trunk_position")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    RELEASE_ID_FIELD_NUMBER: _ClassVar[int]
    TRUNK_POSITION_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    release_id: str
    trunk_position: int
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., release_id: _Optional[str] = ..., trunk_position: _Optional[int] = ...) -> None: ...

class CreateReleaseResponse(_message.Message):
    __slots__ = ("ref",)
    REF_FIELD_NUMBER: _ClassVar[int]
    ref: _types_pb2.Ref
    def __init__(self, ref: _Optional[_Union[_types_pb2.Ref, _Mapping]] = ...) -> None: ...

class GetTrunkPositionRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetTrunkPositionResponse(_message.Message):
    __slots__ = ("landing",)
    LANDING_FIELD_NUMBER: _ClassVar[int]
    landing: _types_pb2.Landing
    def __init__(self, landing: _Optional[_Union[_types_pb2.Landing, _Mapping]] = ...) -> None: ...

class ListTrunkPositionsRequest(_message.Message):
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

class ListTrunkPositionsResponse(_message.Message):
    __slots__ = ("landings", "next_page_token")
    LANDINGS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    landings: _containers.RepeatedCompositeFieldContainer[_types_pb2.Landing]
    next_page_token: str
    def __init__(self, landings: _Optional[_Iterable[_Union[_types_pb2.Landing, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class WatchTrunkRequest(_message.Message):
    __slots__ = ("parent", "after_position")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    AFTER_POSITION_FIELD_NUMBER: _ClassVar[int]
    parent: str
    after_position: int
    def __init__(self, parent: _Optional[str] = ..., after_position: _Optional[int] = ...) -> None: ...

class WatchTrunkResponse(_message.Message):
    __slots__ = ("landing",)
    LANDING_FIELD_NUMBER: _ClassVar[int]
    landing: _types_pb2.Landing
    def __init__(self, landing: _Optional[_Union[_types_pb2.Landing, _Mapping]] = ...) -> None: ...

class WatchRefsRequest(_message.Message):
    __slots__ = ("parent", "prefix")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PREFIX_FIELD_NUMBER: _ClassVar[int]
    parent: str
    prefix: str
    def __init__(self, parent: _Optional[str] = ..., prefix: _Optional[str] = ...) -> None: ...

class WatchRefsResponse(_message.Message):
    __slots__ = ("ref", "old", "new", "at")
    REF_FIELD_NUMBER: _ClassVar[int]
    OLD_FIELD_NUMBER: _ClassVar[int]
    NEW_FIELD_NUMBER: _ClassVar[int]
    AT_FIELD_NUMBER: _ClassVar[int]
    ref: str
    old: bytes
    new: bytes
    at: int
    def __init__(self, ref: _Optional[str] = ..., old: _Optional[bytes] = ..., new: _Optional[bytes] = ..., at: _Optional[int] = ...) -> None: ...

class WatchVersionsRequest(_message.Message):
    __slots__ = ("parent", "after")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    AFTER_FIELD_NUMBER: _ClassVar[int]
    parent: str
    after: bytes
    def __init__(self, parent: _Optional[str] = ..., after: _Optional[bytes] = ...) -> None: ...

class WatchVersionsResponse(_message.Message):
    __slots__ = ("cursor", "version")
    CURSOR_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    cursor: bytes
    version: _types_pb2.ChangeInfo
    def __init__(self, cursor: _Optional[bytes] = ..., version: _Optional[_Union[_types_pb2.ChangeInfo, _Mapping]] = ...) -> None: ...

class GetDerivedValueRequest(_message.Message):
    __slots__ = ("repo", "kind", "hash")
    REPO_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    HASH_FIELD_NUMBER: _ClassVar[int]
    repo: str
    kind: str
    hash: bytes
    def __init__(self, repo: _Optional[str] = ..., kind: _Optional[str] = ..., hash: _Optional[bytes] = ...) -> None: ...

class GetDerivedValueResponse(_message.Message):
    __slots__ = ("data",)
    DATA_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    def __init__(self, data: _Optional[bytes] = ...) -> None: ...

class WaitDerivedValueRequest(_message.Message):
    __slots__ = ("repo", "kind", "hash", "timeout_ms")
    REPO_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    HASH_FIELD_NUMBER: _ClassVar[int]
    TIMEOUT_MS_FIELD_NUMBER: _ClassVar[int]
    repo: str
    kind: str
    hash: bytes
    timeout_ms: int
    def __init__(self, repo: _Optional[str] = ..., kind: _Optional[str] = ..., hash: _Optional[bytes] = ..., timeout_ms: _Optional[int] = ...) -> None: ...

class WaitDerivedValueResponse(_message.Message):
    __slots__ = ("ready", "data")
    READY_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    ready: bool
    data: bytes
    def __init__(self, ready: _Optional[bool] = ..., data: _Optional[bytes] = ...) -> None: ...
