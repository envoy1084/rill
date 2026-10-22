from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class VersionView(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    VERSION_VIEW_UNSPECIFIED: _ClassVar[VersionView]
    VERSION_VIEW_BASIC: _ClassVar[VersionView]
    VERSION_VIEW_FULL: _ClassVar[VersionView]
VERSION_VIEW_UNSPECIFIED: VersionView
VERSION_VIEW_BASIC: VersionView
VERSION_VIEW_FULL: VersionView

class Version(_message.Message):
    __slots__ = ("info", "version", "signatures", "read_grant")
    INFO_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    SIGNATURES_FIELD_NUMBER: _ClassVar[int]
    READ_GRANT_FIELD_NUMBER: _ClassVar[int]
    info: _types_pb2.ChangeInfo
    version: _types_pb2.ChangeVersion
    signatures: _containers.RepeatedCompositeFieldContainer[_types_pb2.Signature]
    read_grant: str
    def __init__(self, info: _Optional[_Union[_types_pb2.ChangeInfo, _Mapping]] = ..., version: _Optional[_Union[_types_pb2.ChangeVersion, _Mapping]] = ..., signatures: _Optional[_Iterable[_Union[_types_pb2.Signature, _Mapping]]] = ..., read_grant: _Optional[str] = ...) -> None: ...

class GetVersionRequest(_message.Message):
    __slots__ = ("name", "view")
    NAME_FIELD_NUMBER: _ClassVar[int]
    VIEW_FIELD_NUMBER: _ClassVar[int]
    name: str
    view: VersionView
    def __init__(self, name: _Optional[str] = ..., view: _Optional[_Union[VersionView, str]] = ...) -> None: ...

class GetVersionResponse(_message.Message):
    __slots__ = ("version",)
    VERSION_FIELD_NUMBER: _ClassVar[int]
    version: Version
    def __init__(self, version: _Optional[_Union[Version, _Mapping]] = ...) -> None: ...

class BatchGetVersionsRequest(_message.Message):
    __slots__ = ("parent", "names", "view")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    NAMES_FIELD_NUMBER: _ClassVar[int]
    VIEW_FIELD_NUMBER: _ClassVar[int]
    parent: str
    names: _containers.RepeatedScalarFieldContainer[str]
    view: VersionView
    def __init__(self, parent: _Optional[str] = ..., names: _Optional[_Iterable[str]] = ..., view: _Optional[_Union[VersionView, str]] = ...) -> None: ...

class BatchGetVersionsResponse(_message.Message):
    __slots__ = ("versions",)
    VERSIONS_FIELD_NUMBER: _ClassVar[int]
    versions: _containers.RepeatedCompositeFieldContainer[Version]
    def __init__(self, versions: _Optional[_Iterable[_Union[Version, _Mapping]]] = ...) -> None: ...

class ListVersionsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListVersionsResponse(_message.Message):
    __slots__ = ("versions", "next_page_token")
    VERSIONS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    versions: _containers.RepeatedCompositeFieldContainer[_types_pb2.ChangeInfo]
    next_page_token: str
    def __init__(self, versions: _Optional[_Iterable[_Union[_types_pb2.ChangeInfo, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class CreateVersionRequest(_message.Message):
    __slots__ = ("request_id", "parent", "version", "signatures")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    SIGNATURES_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    version: _types_pb2.ChangeVersion
    signatures: _containers.RepeatedCompositeFieldContainer[_types_pb2.Signature]
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., version: _Optional[_Union[_types_pb2.ChangeVersion, _Mapping]] = ..., signatures: _Optional[_Iterable[_Union[_types_pb2.Signature, _Mapping]]] = ...) -> None: ...

class CreateVersionResponse(_message.Message):
    __slots__ = ("version",)
    VERSION_FIELD_NUMBER: _ClassVar[int]
    version: _types_pb2.ChangeInfo
    def __init__(self, version: _Optional[_Union[_types_pb2.ChangeInfo, _Mapping]] = ...) -> None: ...

class EditBase(_message.Message):
    __slots__ = ("revision", "change")
    REVISION_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    revision: str
    change: str
    def __init__(self, revision: _Optional[str] = ..., change: _Optional[str] = ...) -> None: ...

class Edit(_message.Message):
    __slots__ = ("path", "set_content", "delete", "patch")
    PATH_FIELD_NUMBER: _ClassVar[int]
    SET_CONTENT_FIELD_NUMBER: _ClassVar[int]
    DELETE_FIELD_NUMBER: _ClassVar[int]
    PATCH_FIELD_NUMBER: _ClassVar[int]
    path: str
    set_content: SetContent
    delete: bool
    patch: str
    def __init__(self, path: _Optional[str] = ..., set_content: _Optional[_Union[SetContent, _Mapping]] = ..., delete: _Optional[bool] = ..., patch: _Optional[str] = ...) -> None: ...

class SetContent(_message.Message):
    __slots__ = ("content", "kind", "copy_from")
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    KIND_FIELD_NUMBER: _ClassVar[int]
    COPY_FROM_FIELD_NUMBER: _ClassVar[int]
    content: _types_pb2.ContentRef
    kind: _types_pb2.FileKind
    copy_from: _types_pb2.CopySource
    def __init__(self, content: _Optional[_Union[_types_pb2.ContentRef, _Mapping]] = ..., kind: _Optional[_Union[_types_pb2.FileKind, str]] = ..., copy_from: _Optional[_Union[_types_pb2.CopySource, _Mapping]] = ...) -> None: ...

class ApplyEditsRequest(_message.Message):
    __slots__ = ("request_id", "parent", "base", "edits", "message", "intent", "provenance", "change")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    BASE_FIELD_NUMBER: _ClassVar[int]
    EDITS_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    PROVENANCE_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    base: EditBase
    edits: _containers.RepeatedCompositeFieldContainer[Edit]
    message: str
    intent: str
    provenance: _types_pb2.Provenance
    change: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., base: _Optional[_Union[EditBase, _Mapping]] = ..., edits: _Optional[_Iterable[_Union[Edit, _Mapping]]] = ..., message: _Optional[str] = ..., intent: _Optional[str] = ..., provenance: _Optional[_Union[_types_pb2.Provenance, _Mapping]] = ..., change: _Optional[str] = ...) -> None: ...

class ApplyEditsResponse(_message.Message):
    __slots__ = ("info", "version")
    INFO_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    info: _types_pb2.ChangeInfo
    version: _types_pb2.ChangeVersion
    def __init__(self, info: _Optional[_Union[_types_pb2.ChangeInfo, _Mapping]] = ..., version: _Optional[_Union[_types_pb2.ChangeVersion, _Mapping]] = ...) -> None: ...

class RebaseVersionRequest(_message.Message):
    __slots__ = ("request_id", "name", "onto")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    ONTO_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    onto: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., onto: _Optional[str] = ...) -> None: ...

class RebaseVersionResponse(_message.Message):
    __slots__ = ("info", "version")
    INFO_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    info: _types_pb2.ChangeInfo
    version: _types_pb2.ChangeVersion
    def __init__(self, info: _Optional[_Union[_types_pb2.ChangeInfo, _Mapping]] = ..., version: _Optional[_Union[_types_pb2.ChangeVersion, _Mapping]] = ...) -> None: ...

class MergeVersionsRequest(_message.Message):
    __slots__ = ("request_id", "parent", "left", "right", "message", "intent", "provenance")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    LEFT_FIELD_NUMBER: _ClassVar[int]
    RIGHT_FIELD_NUMBER: _ClassVar[int]
    MESSAGE_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    PROVENANCE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    left: str
    right: str
    message: str
    intent: str
    provenance: _types_pb2.Provenance
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., left: _Optional[str] = ..., right: _Optional[str] = ..., message: _Optional[str] = ..., intent: _Optional[str] = ..., provenance: _Optional[_Union[_types_pb2.Provenance, _Mapping]] = ...) -> None: ...

class MergeVersionsResponse(_message.Message):
    __slots__ = ("info", "version")
    INFO_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    info: _types_pb2.ChangeInfo
    version: _types_pb2.ChangeVersion
    def __init__(self, info: _Optional[_Union[_types_pb2.ChangeInfo, _Mapping]] = ..., version: _Optional[_Union[_types_pb2.ChangeVersion, _Mapping]] = ...) -> None: ...

class Resolution(_message.Message):
    __slots__ = ("path", "set", "delete")
    PATH_FIELD_NUMBER: _ClassVar[int]
    SET_FIELD_NUMBER: _ClassVar[int]
    DELETE_FIELD_NUMBER: _ClassVar[int]
    path: str
    set: SetContent
    delete: bool
    def __init__(self, path: _Optional[str] = ..., set: _Optional[_Union[SetContent, _Mapping]] = ..., delete: _Optional[bool] = ...) -> None: ...

class ResolveConflictsRequest(_message.Message):
    __slots__ = ("request_id", "name", "resolutions", "provenance")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    RESOLUTIONS_FIELD_NUMBER: _ClassVar[int]
    PROVENANCE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    resolutions: _containers.RepeatedCompositeFieldContainer[Resolution]
    provenance: _types_pb2.Provenance
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ..., resolutions: _Optional[_Iterable[_Union[Resolution, _Mapping]]] = ..., provenance: _Optional[_Union[_types_pb2.Provenance, _Mapping]] = ...) -> None: ...

class ResolveConflictsResponse(_message.Message):
    __slots__ = ("info", "version")
    INFO_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    info: _types_pb2.ChangeInfo
    version: _types_pb2.ChangeVersion
    def __init__(self, info: _Optional[_Union[_types_pb2.ChangeInfo, _Mapping]] = ..., version: _Optional[_Union[_types_pb2.ChangeVersion, _Mapping]] = ...) -> None: ...
