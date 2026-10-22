from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ImportTrunkPositionRequest(_message.Message):
    __slots__ = ("request_id", "parent", "expected", "version")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    EXPECTED_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    expected: bytes
    version: bytes
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., expected: _Optional[bytes] = ..., version: _Optional[bytes] = ...) -> None: ...

class ImportTrunkPositionResponse(_message.Message):
    __slots__ = ("position",)
    POSITION_FIELD_NUMBER: _ClassVar[int]
    position: int
    def __init__(self, position: _Optional[int] = ...) -> None: ...

class GetHealthRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class ComponentHealth(_message.Message):
    __slots__ = ("name", "ok", "detail")
    NAME_FIELD_NUMBER: _ClassVar[int]
    OK_FIELD_NUMBER: _ClassVar[int]
    DETAIL_FIELD_NUMBER: _ClassVar[int]
    name: str
    ok: bool
    detail: str
    def __init__(self, name: _Optional[str] = ..., ok: _Optional[bool] = ..., detail: _Optional[str] = ...) -> None: ...

class GetHealthResponse(_message.Message):
    __slots__ = ("ok", "version", "components", "attestation_key")
    OK_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    COMPONENTS_FIELD_NUMBER: _ClassVar[int]
    ATTESTATION_KEY_FIELD_NUMBER: _ClassVar[int]
    ok: bool
    version: str
    components: _containers.RepeatedCompositeFieldContainer[ComponentHealth]
    attestation_key: str
    def __init__(self, ok: _Optional[bool] = ..., version: _Optional[str] = ..., components: _Optional[_Iterable[_Union[ComponentHealth, _Mapping]]] = ..., attestation_key: _Optional[str] = ...) -> None: ...
