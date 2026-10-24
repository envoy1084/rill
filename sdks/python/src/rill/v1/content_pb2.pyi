from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ReadFileRequest(_message.Message):
    __slots__ = ("repo", "revision", "path", "max_bytes")
    REPO_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    MAX_BYTES_FIELD_NUMBER: _ClassVar[int]
    repo: str
    revision: str
    path: str
    max_bytes: int
    def __init__(self, repo: _Optional[str] = ..., revision: _Optional[str] = ..., path: _Optional[str] = ..., max_bytes: _Optional[int] = ...) -> None: ...

class ReadFileResponse(_message.Message):
    __slots__ = ("entry", "data", "truncated", "read_grant")
    ENTRY_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    TRUNCATED_FIELD_NUMBER: _ClassVar[int]
    READ_GRANT_FIELD_NUMBER: _ClassVar[int]
    entry: _types_pb2.TreeEntry
    data: bytes
    truncated: bool
    read_grant: str
    def __init__(self, entry: _Optional[_Union[_types_pb2.TreeEntry, _Mapping]] = ..., data: _Optional[bytes] = ..., truncated: _Optional[bool] = ..., read_grant: _Optional[str] = ...) -> None: ...

class BatchReadFilesRequest(_message.Message):
    __slots__ = ("repo", "revision", "paths", "max_bytes")
    REPO_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    PATHS_FIELD_NUMBER: _ClassVar[int]
    MAX_BYTES_FIELD_NUMBER: _ClassVar[int]
    repo: str
    revision: str
    paths: _containers.RepeatedScalarFieldContainer[str]
    max_bytes: int
    def __init__(self, repo: _Optional[str] = ..., revision: _Optional[str] = ..., paths: _Optional[_Iterable[str]] = ..., max_bytes: _Optional[int] = ...) -> None: ...

class BatchReadFilesResponse(_message.Message):
    __slots__ = ("files",)
    FILES_FIELD_NUMBER: _ClassVar[int]
    files: _containers.RepeatedCompositeFieldContainer[ReadFileResponse]
    def __init__(self, files: _Optional[_Iterable[_Union[ReadFileResponse, _Mapping]]] = ...) -> None: ...

class StatRequest(_message.Message):
    __slots__ = ("repo", "revision", "path")
    REPO_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    repo: str
    revision: str
    path: str
    def __init__(self, repo: _Optional[str] = ..., revision: _Optional[str] = ..., path: _Optional[str] = ...) -> None: ...

class StatResponse(_message.Message):
    __slots__ = ("entry", "read_grant")
    ENTRY_FIELD_NUMBER: _ClassVar[int]
    READ_GRANT_FIELD_NUMBER: _ClassVar[int]
    entry: _types_pb2.TreeEntry
    read_grant: str
    def __init__(self, entry: _Optional[_Union[_types_pb2.TreeEntry, _Mapping]] = ..., read_grant: _Optional[str] = ...) -> None: ...

class ListDirectoryRequest(_message.Message):
    __slots__ = ("repo", "revision", "path", "page_size", "page_token")
    REPO_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    repo: str
    revision: str
    path: str
    page_size: int
    page_token: str
    def __init__(self, repo: _Optional[str] = ..., revision: _Optional[str] = ..., path: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListDirectoryResponse(_message.Message):
    __slots__ = ("tree_hash", "entries", "next_page_token", "read_grant")
    TREE_HASH_FIELD_NUMBER: _ClassVar[int]
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    READ_GRANT_FIELD_NUMBER: _ClassVar[int]
    tree_hash: bytes
    entries: _containers.RepeatedCompositeFieldContainer[_types_pb2.NamedTreeEntry]
    next_page_token: str
    read_grant: str
    def __init__(self, tree_hash: _Optional[bytes] = ..., entries: _Optional[_Iterable[_Union[_types_pb2.NamedTreeEntry, _Mapping]]] = ..., next_page_token: _Optional[str] = ..., read_grant: _Optional[str] = ...) -> None: ...

class GetTreeRequest(_message.Message):
    __slots__ = ("repo", "hash", "grant")
    REPO_FIELD_NUMBER: _ClassVar[int]
    HASH_FIELD_NUMBER: _ClassVar[int]
    GRANT_FIELD_NUMBER: _ClassVar[int]
    repo: str
    hash: bytes
    grant: str
    def __init__(self, repo: _Optional[str] = ..., hash: _Optional[bytes] = ..., grant: _Optional[str] = ...) -> None: ...

class GetTreeResponse(_message.Message):
    __slots__ = ("tree", "read_grant")
    TREE_FIELD_NUMBER: _ClassVar[int]
    READ_GRANT_FIELD_NUMBER: _ClassVar[int]
    tree: _types_pb2.Tree
    read_grant: str
    def __init__(self, tree: _Optional[_Union[_types_pb2.Tree, _Mapping]] = ..., read_grant: _Optional[str] = ...) -> None: ...

class ReadBlobRequest(_message.Message):
    __slots__ = ("repo", "content", "offset", "length", "grant")
    REPO_FIELD_NUMBER: _ClassVar[int]
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    OFFSET_FIELD_NUMBER: _ClassVar[int]
    LENGTH_FIELD_NUMBER: _ClassVar[int]
    GRANT_FIELD_NUMBER: _ClassVar[int]
    repo: str
    content: _types_pb2.ContentRef
    offset: int
    length: int
    grant: str
    def __init__(self, repo: _Optional[str] = ..., content: _Optional[_Union[_types_pb2.ContentRef, _Mapping]] = ..., offset: _Optional[int] = ..., length: _Optional[int] = ..., grant: _Optional[str] = ...) -> None: ...

class ReadBlobResponse(_message.Message):
    __slots__ = ("data",)
    DATA_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    def __init__(self, data: _Optional[bytes] = ...) -> None: ...

class WriteBlobRequest(_message.Message):
    __slots__ = ("header", "data")
    HEADER_FIELD_NUMBER: _ClassVar[int]
    DATA_FIELD_NUMBER: _ClassVar[int]
    header: WriteBlobHeader
    data: bytes
    def __init__(self, header: _Optional[_Union[WriteBlobHeader, _Mapping]] = ..., data: _Optional[bytes] = ...) -> None: ...

class WriteBlobHeader(_message.Message):
    __slots__ = ("request_id", "repo", "size", "expected_hash")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    REPO_FIELD_NUMBER: _ClassVar[int]
    SIZE_FIELD_NUMBER: _ClassVar[int]
    EXPECTED_HASH_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    repo: str
    size: int
    expected_hash: bytes
    def __init__(self, request_id: _Optional[str] = ..., repo: _Optional[str] = ..., size: _Optional[int] = ..., expected_hash: _Optional[bytes] = ...) -> None: ...

class WriteBlobResponse(_message.Message):
    __slots__ = ("content",)
    CONTENT_FIELD_NUMBER: _ClassVar[int]
    content: _types_pb2.ContentRef
    def __init__(self, content: _Optional[_Union[_types_pb2.ContentRef, _Mapping]] = ...) -> None: ...

class BatchCheckObjectsRequest(_message.Message):
    __slots__ = ("repo", "objects")
    REPO_FIELD_NUMBER: _ClassVar[int]
    OBJECTS_FIELD_NUMBER: _ClassVar[int]
    repo: str
    objects: _containers.RepeatedCompositeFieldContainer[ObjectRef]
    def __init__(self, repo: _Optional[str] = ..., objects: _Optional[_Iterable[_Union[ObjectRef, _Mapping]]] = ...) -> None: ...

class ObjectRef(_message.Message):
    __slots__ = ("kind", "hash")
    KIND_FIELD_NUMBER: _ClassVar[int]
    HASH_FIELD_NUMBER: _ClassVar[int]
    kind: _types_pb2.ObjectKind
    hash: bytes
    def __init__(self, kind: _Optional[_Union[_types_pb2.ObjectKind, str]] = ..., hash: _Optional[bytes] = ...) -> None: ...

class BatchCheckObjectsResponse(_message.Message):
    __slots__ = ("present",)
    PRESENT_FIELD_NUMBER: _ClassVar[int]
    present: _containers.RepeatedScalarFieldContainer[bool]
    def __init__(self, present: _Optional[_Iterable[bool]] = ...) -> None: ...

class DiffRequest(_message.Message):
    __slots__ = ("repo", "base", "target", "paths", "format", "context_lines", "max_files")
    REPO_FIELD_NUMBER: _ClassVar[int]
    BASE_FIELD_NUMBER: _ClassVar[int]
    TARGET_FIELD_NUMBER: _ClassVar[int]
    PATHS_FIELD_NUMBER: _ClassVar[int]
    FORMAT_FIELD_NUMBER: _ClassVar[int]
    CONTEXT_LINES_FIELD_NUMBER: _ClassVar[int]
    MAX_FILES_FIELD_NUMBER: _ClassVar[int]
    repo: str
    base: str
    target: str
    paths: _containers.RepeatedScalarFieldContainer[str]
    format: _types_pb2.DiffFormat
    context_lines: int
    max_files: int
    def __init__(self, repo: _Optional[str] = ..., base: _Optional[str] = ..., target: _Optional[str] = ..., paths: _Optional[_Iterable[str]] = ..., format: _Optional[_Union[_types_pb2.DiffFormat, str]] = ..., context_lines: _Optional[int] = ..., max_files: _Optional[int] = ...) -> None: ...

class DiffResponse(_message.Message):
    __slots__ = ("files", "truncated", "read_grant")
    FILES_FIELD_NUMBER: _ClassVar[int]
    TRUNCATED_FIELD_NUMBER: _ClassVar[int]
    READ_GRANT_FIELD_NUMBER: _ClassVar[int]
    files: _containers.RepeatedCompositeFieldContainer[_types_pb2.FileDiff]
    truncated: bool
    read_grant: str
    def __init__(self, files: _Optional[_Iterable[_Union[_types_pb2.FileDiff, _Mapping]]] = ..., truncated: _Optional[bool] = ..., read_grant: _Optional[str] = ...) -> None: ...

class BlameRequest(_message.Message):
    __slots__ = ("repo", "revision", "path")
    REPO_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    repo: str
    revision: str
    path: str
    def __init__(self, repo: _Optional[str] = ..., revision: _Optional[str] = ..., path: _Optional[str] = ...) -> None: ...

class BlameResponse(_message.Message):
    __slots__ = ("lines",)
    LINES_FIELD_NUMBER: _ClassVar[int]
    lines: _containers.RepeatedCompositeFieldContainer[BlameLine]
    def __init__(self, lines: _Optional[_Iterable[_Union[BlameLine, _Mapping]]] = ...) -> None: ...

class BlameLine(_message.Message):
    __slots__ = ("line_no", "version", "author", "trunk_position", "text")
    LINE_NO_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    AUTHOR_FIELD_NUMBER: _ClassVar[int]
    TRUNK_POSITION_FIELD_NUMBER: _ClassVar[int]
    TEXT_FIELD_NUMBER: _ClassVar[int]
    line_no: int
    version: str
    author: _types_pb2.Principal
    trunk_position: int
    text: bytes
    def __init__(self, line_no: _Optional[int] = ..., version: _Optional[str] = ..., author: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., trunk_position: _Optional[int] = ..., text: _Optional[bytes] = ...) -> None: ...

class ListHistoryRequest(_message.Message):
    __slots__ = ("repo", "revision", "path", "first_parent", "page_size", "page_token")
    REPO_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    FIRST_PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    repo: str
    revision: str
    path: str
    first_parent: bool
    page_size: int
    page_token: str
    def __init__(self, repo: _Optional[str] = ..., revision: _Optional[str] = ..., path: _Optional[str] = ..., first_parent: _Optional[bool] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListHistoryResponse(_message.Message):
    __slots__ = ("versions", "next_page_token")
    VERSIONS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    versions: _containers.RepeatedCompositeFieldContainer[_types_pb2.ChangeInfo]
    next_page_token: str
    def __init__(self, versions: _Optional[_Iterable[_Union[_types_pb2.ChangeInfo, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class ListPathHistoryRequest(_message.Message):
    __slots__ = ("repo", "revision", "path", "page_size", "page_token")
    REPO_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    repo: str
    revision: str
    path: str
    page_size: int
    page_token: str
    def __init__(self, repo: _Optional[str] = ..., revision: _Optional[str] = ..., path: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListPathHistoryResponse(_message.Message):
    __slots__ = ("entries", "next_page_token")
    ENTRIES_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    entries: _containers.RepeatedCompositeFieldContainer[PathHistoryEntry]
    next_page_token: str
    def __init__(self, entries: _Optional[_Iterable[_Union[PathHistoryEntry, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class PathHistoryEntry(_message.Message):
    __slots__ = ("version", "path", "change")
    VERSION_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    version: _types_pb2.ChangeInfo
    path: str
    change: _types_pb2.FileChange
    def __init__(self, version: _Optional[_Union[_types_pb2.ChangeInfo, _Mapping]] = ..., path: _Optional[str] = ..., change: _Optional[_Union[_types_pb2.FileChange, _Mapping]] = ...) -> None: ...

class GrepRequest(_message.Message):
    __slots__ = ("repo", "revision", "pattern", "case_insensitive", "path_globs", "max_results", "context_lines", "literal", "languages", "min_seq")
    REPO_FIELD_NUMBER: _ClassVar[int]
    REVISION_FIELD_NUMBER: _ClassVar[int]
    PATTERN_FIELD_NUMBER: _ClassVar[int]
    CASE_INSENSITIVE_FIELD_NUMBER: _ClassVar[int]
    PATH_GLOBS_FIELD_NUMBER: _ClassVar[int]
    MAX_RESULTS_FIELD_NUMBER: _ClassVar[int]
    CONTEXT_LINES_FIELD_NUMBER: _ClassVar[int]
    LITERAL_FIELD_NUMBER: _ClassVar[int]
    LANGUAGES_FIELD_NUMBER: _ClassVar[int]
    MIN_SEQ_FIELD_NUMBER: _ClassVar[int]
    repo: str
    revision: str
    pattern: str
    case_insensitive: bool
    path_globs: _containers.RepeatedScalarFieldContainer[str]
    max_results: int
    context_lines: int
    literal: bool
    languages: _containers.RepeatedScalarFieldContainer[str]
    min_seq: int
    def __init__(self, repo: _Optional[str] = ..., revision: _Optional[str] = ..., pattern: _Optional[str] = ..., case_insensitive: _Optional[bool] = ..., path_globs: _Optional[_Iterable[str]] = ..., max_results: _Optional[int] = ..., context_lines: _Optional[int] = ..., literal: _Optional[bool] = ..., languages: _Optional[_Iterable[str]] = ..., min_seq: _Optional[int] = ...) -> None: ...

class GrepResponse(_message.Message):
    __slots__ = ("hits", "truncated", "indexed_through", "files_matched", "partial", "directories", "languages")
    class DirectoriesEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: int
        def __init__(self, key: _Optional[str] = ..., value: _Optional[int] = ...) -> None: ...
    class LanguagesEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: int
        def __init__(self, key: _Optional[str] = ..., value: _Optional[int] = ...) -> None: ...
    HITS_FIELD_NUMBER: _ClassVar[int]
    TRUNCATED_FIELD_NUMBER: _ClassVar[int]
    INDEXED_THROUGH_FIELD_NUMBER: _ClassVar[int]
    FILES_MATCHED_FIELD_NUMBER: _ClassVar[int]
    PARTIAL_FIELD_NUMBER: _ClassVar[int]
    DIRECTORIES_FIELD_NUMBER: _ClassVar[int]
    LANGUAGES_FIELD_NUMBER: _ClassVar[int]
    hits: _containers.RepeatedCompositeFieldContainer[GrepHit]
    truncated: bool
    indexed_through: int
    files_matched: int
    partial: bool
    directories: _containers.ScalarMap[str, int]
    languages: _containers.ScalarMap[str, int]
    def __init__(self, hits: _Optional[_Iterable[_Union[GrepHit, _Mapping]]] = ..., truncated: _Optional[bool] = ..., indexed_through: _Optional[int] = ..., files_matched: _Optional[int] = ..., partial: _Optional[bool] = ..., directories: _Optional[_Mapping[str, int]] = ..., languages: _Optional[_Mapping[str, int]] = ...) -> None: ...

class GrepHit(_message.Message):
    __slots__ = ("path", "line_no", "line", "before", "after", "byte_start", "byte_end")
    PATH_FIELD_NUMBER: _ClassVar[int]
    LINE_NO_FIELD_NUMBER: _ClassVar[int]
    LINE_FIELD_NUMBER: _ClassVar[int]
    BEFORE_FIELD_NUMBER: _ClassVar[int]
    AFTER_FIELD_NUMBER: _ClassVar[int]
    BYTE_START_FIELD_NUMBER: _ClassVar[int]
    BYTE_END_FIELD_NUMBER: _ClassVar[int]
    path: str
    line_no: int
    line: str
    before: _containers.RepeatedScalarFieldContainer[str]
    after: _containers.RepeatedScalarFieldContainer[str]
    byte_start: int
    byte_end: int
    def __init__(self, path: _Optional[str] = ..., line_no: _Optional[int] = ..., line: _Optional[str] = ..., before: _Optional[_Iterable[str]] = ..., after: _Optional[_Iterable[str]] = ..., byte_start: _Optional[int] = ..., byte_end: _Optional[int] = ...) -> None: ...

class FindMergeBaseRequest(_message.Message):
    __slots__ = ("repo", "a", "b")
    REPO_FIELD_NUMBER: _ClassVar[int]
    A_FIELD_NUMBER: _ClassVar[int]
    B_FIELD_NUMBER: _ClassVar[int]
    repo: str
    a: str
    b: str
    def __init__(self, repo: _Optional[str] = ..., a: _Optional[str] = ..., b: _Optional[str] = ...) -> None: ...

class FindMergeBaseResponse(_message.Message):
    __slots__ = ("bases",)
    BASES_FIELD_NUMBER: _ClassVar[int]
    bases: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, bases: _Optional[_Iterable[str]] = ...) -> None: ...

class IsAncestorRequest(_message.Message):
    __slots__ = ("repo", "ancestor", "descendant")
    REPO_FIELD_NUMBER: _ClassVar[int]
    ANCESTOR_FIELD_NUMBER: _ClassVar[int]
    DESCENDANT_FIELD_NUMBER: _ClassVar[int]
    repo: str
    ancestor: str
    descendant: str
    def __init__(self, repo: _Optional[str] = ..., ancestor: _Optional[str] = ..., descendant: _Optional[str] = ...) -> None: ...

class IsAncestorResponse(_message.Message):
    __slots__ = ("is_ancestor",)
    IS_ANCESTOR_FIELD_NUMBER: _ClassVar[int]
    is_ancestor: bool
    def __init__(self, is_ancestor: _Optional[bool] = ...) -> None: ...
