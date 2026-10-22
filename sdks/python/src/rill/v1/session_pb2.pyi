from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class ListSignInProvidersRequest(_message.Message):
    __slots__ = ("parent",)
    PARENT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    def __init__(self, parent: _Optional[str] = ...) -> None: ...

class SignInProvider(_message.Message):
    __slots__ = ("org", "name", "issuer", "cli_client_id", "scopes", "device_authorization_endpoint", "token_endpoint")
    ORG_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    ISSUER_FIELD_NUMBER: _ClassVar[int]
    CLI_CLIENT_ID_FIELD_NUMBER: _ClassVar[int]
    SCOPES_FIELD_NUMBER: _ClassVar[int]
    DEVICE_AUTHORIZATION_ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    TOKEN_ENDPOINT_FIELD_NUMBER: _ClassVar[int]
    org: str
    name: str
    issuer: str
    cli_client_id: str
    scopes: _containers.RepeatedScalarFieldContainer[str]
    device_authorization_endpoint: str
    token_endpoint: str
    def __init__(self, org: _Optional[str] = ..., name: _Optional[str] = ..., issuer: _Optional[str] = ..., cli_client_id: _Optional[str] = ..., scopes: _Optional[_Iterable[str]] = ..., device_authorization_endpoint: _Optional[str] = ..., token_endpoint: _Optional[str] = ...) -> None: ...

class ListSignInProvidersResponse(_message.Message):
    __slots__ = ("providers",)
    PROVIDERS_FIELD_NUMBER: _ClassVar[int]
    providers: _containers.RepeatedCompositeFieldContainer[SignInProvider]
    def __init__(self, providers: _Optional[_Iterable[_Union[SignInProvider, _Mapping]]] = ...) -> None: ...

class ExchangeTokenRequest(_message.Message):
    __slots__ = ("subject_token", "subject_token_type", "org", "agent", "model")
    SUBJECT_TOKEN_FIELD_NUMBER: _ClassVar[int]
    SUBJECT_TOKEN_TYPE_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    AGENT_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    subject_token: str
    subject_token_type: str
    org: str
    agent: str
    model: str
    def __init__(self, subject_token: _Optional[str] = ..., subject_token_type: _Optional[str] = ..., org: _Optional[str] = ..., agent: _Optional[str] = ..., model: _Optional[str] = ...) -> None: ...

class ExchangeTokenResponse(_message.Message):
    __slots__ = ("grant",)
    GRANT_FIELD_NUMBER: _ClassVar[int]
    grant: SessionGrant
    def __init__(self, grant: _Optional[_Union[SessionGrant, _Mapping]] = ...) -> None: ...

class RefreshSessionRequest(_message.Message):
    __slots__ = ("refresh_token",)
    REFRESH_TOKEN_FIELD_NUMBER: _ClassVar[int]
    refresh_token: str
    def __init__(self, refresh_token: _Optional[str] = ...) -> None: ...

class RefreshSessionResponse(_message.Message):
    __slots__ = ("grant",)
    GRANT_FIELD_NUMBER: _ClassVar[int]
    grant: SessionGrant
    def __init__(self, grant: _Optional[_Union[SessionGrant, _Mapping]] = ...) -> None: ...

class SessionGrant(_message.Message):
    __slots__ = ("access_token", "expires_at", "refresh_token", "session", "principal", "org")
    ACCESS_TOKEN_FIELD_NUMBER: _ClassVar[int]
    EXPIRES_AT_FIELD_NUMBER: _ClassVar[int]
    REFRESH_TOKEN_FIELD_NUMBER: _ClassVar[int]
    SESSION_FIELD_NUMBER: _ClassVar[int]
    PRINCIPAL_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    access_token: str
    expires_at: int
    refresh_token: str
    session: str
    principal: _types_pb2.Principal
    org: str
    def __init__(self, access_token: _Optional[str] = ..., expires_at: _Optional[int] = ..., refresh_token: _Optional[str] = ..., session: _Optional[str] = ..., principal: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., org: _Optional[str] = ...) -> None: ...
