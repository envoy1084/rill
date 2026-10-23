from rill.v1 import changes_pb2 as _changes_pb2
from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class CreateWorkspaceRequest(_message.Message):
    __slots__ = ("request_id", "parent", "workspace", "base")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    WORKSPACE_FIELD_NUMBER: _ClassVar[int]
    BASE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    workspace: _types_pb2.Workspace
    base: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., workspace: _Optional[_Union[_types_pb2.Workspace, _Mapping]] = ..., base: _Optional[str] = ...) -> None: ...

class CreateWorkspaceResponse(_message.Message):
    __slots__ = ("workspace", "operation")
    WORKSPACE_FIELD_NUMBER: _ClassVar[int]
    OPERATION_FIELD_NUMBER: _ClassVar[int]
    workspace: _types_pb2.Workspace
    operation: _types_pb2.WorkspaceOperation
    def __init__(self, workspace: _Optional[_Union[_types_pb2.Workspace, _Mapping]] = ..., operation: _Optional[_Union[_types_pb2.WorkspaceOperation, _Mapping]] = ...) -> None: ...

class GetWorkspaceRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetWorkspaceResponse(_message.Message):
    __slots__ = ("workspace",)
    WORKSPACE_FIELD_NUMBER: _ClassVar[int]
    workspace: _types_pb2.Workspace
    def __init__(self, workspace: _Optional[_Union[_types_pb2.Workspace, _Mapping]] = ...) -> None: ...

class ListWorkspacesRequest(_message.Message):
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

class ListWorkspacesResponse(_message.Message):
    __slots__ = ("workspaces", "next_page_token")
    WORKSPACES_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    workspaces: _containers.RepeatedCompositeFieldContainer[_types_pb2.Workspace]
    next_page_token: str
    def __init__(self, workspaces: _Optional[_Iterable[_Union[_types_pb2.Workspace, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class UpdateWorkspaceRequest(_message.Message):
    __slots__ = ("request_id", "workspace", "etag")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    WORKSPACE_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    workspace: _types_pb2.Workspace
    etag: str
    def __init__(self, request_id: _Optional[str] = ..., workspace: _Optional[_Union[_types_pb2.Workspace, _Mapping]] = ..., etag: _Optional[str] = ...) -> None: ...

class UpdateWorkspaceResponse(_message.Message):
    __slots__ = ("workspace", "operation")
    WORKSPACE_FIELD_NUMBER: _ClassVar[int]
    OPERATION_FIELD_NUMBER: _ClassVar[int]
    workspace: _types_pb2.Workspace
    operation: _types_pb2.WorkspaceOperation
    def __init__(self, workspace: _Optional[_Union[_types_pb2.Workspace, _Mapping]] = ..., operation: _Optional[_Union[_types_pb2.WorkspaceOperation, _Mapping]] = ...) -> None: ...

class DeleteWorkspaceRequest(_message.Message):
    __slots__ = ("request_id", "name", "etag")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    etag: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., etag: _Optional[str] = ...) -> None: ...

class DeleteWorkspaceResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class SnapshotWorkspaceRequest(_message.Message):
    __slots__ = ("request_id", "name", "tree_hash", "edits", "etag")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    TREE_HASH_FIELD_NUMBER: _ClassVar[int]
    EDITS_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    tree_hash: bytes
    edits: WorkspaceEdits
    etag: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., tree_hash: _Optional[bytes] = ..., edits: _Optional[_Union[WorkspaceEdits, _Mapping]] = ..., etag: _Optional[str] = ...) -> None: ...

class WorkspaceEdits(_message.Message):
    __slots__ = ("from_base", "edits")
    FROM_BASE_FIELD_NUMBER: _ClassVar[int]
    EDITS_FIELD_NUMBER: _ClassVar[int]
    from_base: bool
    edits: _containers.RepeatedCompositeFieldContainer[_changes_pb2.Edit]
    def __init__(self, from_base: _Optional[bool] = ..., edits: _Optional[_Iterable[_Union[_changes_pb2.Edit, _Mapping]]] = ...) -> None: ...

class SnapshotWorkspaceResponse(_message.Message):
    __slots__ = ("operation",)
    OPERATION_FIELD_NUMBER: _ClassVar[int]
    operation: _types_pb2.WorkspaceOperation
    def __init__(self, operation: _Optional[_Union[_types_pb2.WorkspaceOperation, _Mapping]] = ...) -> None: ...

class StartChangeRequest(_message.Message):
    __slots__ = ("request_id", "name", "message", "intent", "provenance")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    PROVENANCE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    message: str
    intent: str
    provenance: _types_pb2.Provenance
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., message: _Optional[str] = ..., intent: _Optional[str] = ..., provenance: _Optional[_Union[_types_pb2.Provenance, _Mapping]] = ...) -> None: ...

class StartChangeResponse(_message.Message):
    __slots__ = ("change", "info", "version", "operation")
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    INFO_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    OPERATION_FIELD_NUMBER: _ClassVar[int]
    change: str
    info: _types_pb2.ChangeInfo
    version: _types_pb2.ChangeVersion
    operation: _types_pb2.WorkspaceOperation
    def __init__(self, change: _Optional[str] = ..., info: _Optional[_Union[_types_pb2.ChangeInfo, _Mapping]] = ..., version: _Optional[_Union[_types_pb2.ChangeVersion, _Mapping]] = ..., operation: _Optional[_Union[_types_pb2.WorkspaceOperation, _Mapping]] = ...) -> None: ...

class SyncWorkspaceRequest(_message.Message):
    __slots__ = ("request_id", "name", "base")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    BASE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    base: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., base: _Optional[str] = ...) -> None: ...

class SyncWorkspaceResponse(_message.Message):
    __slots__ = ("workspace", "operation", "info", "version")
    WORKSPACE_FIELD_NUMBER: _ClassVar[int]
    OPERATION_FIELD_NUMBER: _ClassVar[int]
    INFO_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    workspace: _types_pb2.Workspace
    operation: _types_pb2.WorkspaceOperation
    info: _types_pb2.ChangeInfo
    version: _types_pb2.ChangeVersion
    def __init__(self, workspace: _Optional[_Union[_types_pb2.Workspace, _Mapping]] = ..., operation: _Optional[_Union[_types_pb2.WorkspaceOperation, _Mapping]] = ..., info: _Optional[_Union[_types_pb2.ChangeInfo, _Mapping]] = ..., version: _Optional[_Union[_types_pb2.ChangeVersion, _Mapping]] = ...) -> None: ...

class HandoffWorkspaceRequest(_message.Message):
    __slots__ = ("request_id", "name", "to")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    TO_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    to: _types_pb2.Principal
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., to: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ...) -> None: ...

class HandoffWorkspaceResponse(_message.Message):
    __slots__ = ("workspace", "operation")
    WORKSPACE_FIELD_NUMBER: _ClassVar[int]
    OPERATION_FIELD_NUMBER: _ClassVar[int]
    workspace: _types_pb2.Workspace
    operation: _types_pb2.WorkspaceOperation
    def __init__(self, workspace: _Optional[_Union[_types_pb2.Workspace, _Mapping]] = ..., operation: _Optional[_Union[_types_pb2.WorkspaceOperation, _Mapping]] = ...) -> None: ...

class UndoOperationRequest(_message.Message):
    __slots__ = ("request_id", "name")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ...) -> None: ...

class UndoOperationResponse(_message.Message):
    __slots__ = ("workspace", "operation")
    WORKSPACE_FIELD_NUMBER: _ClassVar[int]
    OPERATION_FIELD_NUMBER: _ClassVar[int]
    workspace: _types_pb2.Workspace
    operation: _types_pb2.WorkspaceOperation
    def __init__(self, workspace: _Optional[_Union[_types_pb2.Workspace, _Mapping]] = ..., operation: _Optional[_Union[_types_pb2.WorkspaceOperation, _Mapping]] = ...) -> None: ...

class ListWorkspaceOperationsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListWorkspaceOperationsResponse(_message.Message):
    __slots__ = ("operations", "next_page_token")
    OPERATIONS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    operations: _containers.RepeatedCompositeFieldContainer[_types_pb2.WorkspaceOperation]
    next_page_token: str
    def __init__(self, operations: _Optional[_Iterable[_Union[_types_pb2.WorkspaceOperation, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class DiffWorkspaceRequest(_message.Message):
    __slots__ = ("name", "paths", "format", "context_lines")
    NAME_FIELD_NUMBER: _ClassVar[int]
    PATHS_FIELD_NUMBER: _ClassVar[int]
    FORMAT_FIELD_NUMBER: _ClassVar[int]
    CONTEXT_LINES_FIELD_NUMBER: _ClassVar[int]
    name: str
    paths: _containers.RepeatedScalarFieldContainer[str]
    format: _types_pb2.DiffFormat
    context_lines: int
    def __init__(self, name: _Optional[str] = ..., paths: _Optional[_Iterable[str]] = ..., format: _Optional[_Union[_types_pb2.DiffFormat, str]] = ..., context_lines: _Optional[int] = ...) -> None: ...

class DiffWorkspaceResponse(_message.Message):
    __slots__ = ("files",)
    FILES_FIELD_NUMBER: _ClassVar[int]
    files: _containers.RepeatedCompositeFieldContainer[_types_pb2.FileDiff]
    def __init__(self, files: _Optional[_Iterable[_Union[_types_pb2.FileDiff, _Mapping]]] = ...) -> None: ...
