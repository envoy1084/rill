from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class Policy(_message.Message):
    __slots__ = ("name", "text", "updated_at", "updated_by", "etag")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TEXT_FIELD_NUMBER: _ClassVar[int]
    UPDATED_AT_FIELD_NUMBER: _ClassVar[int]
    UPDATED_BY_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    name: str
    text: str
    updated_at: int
    updated_by: _types_pb2.Principal
    etag: str
    def __init__(self, name: _Optional[str] = ..., text: _Optional[str] = ..., updated_at: _Optional[int] = ..., updated_by: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., etag: _Optional[str] = ...) -> None: ...

class GetPolicyRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetPolicyResponse(_message.Message):
    __slots__ = ("policy",)
    POLICY_FIELD_NUMBER: _ClassVar[int]
    policy: Policy
    def __init__(self, policy: _Optional[_Union[Policy, _Mapping]] = ...) -> None: ...

class UpdatePolicyRequest(_message.Message):
    __slots__ = ("request_id", "policy", "etag")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    POLICY_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    policy: Policy
    etag: str
    def __init__(self, request_id: _Optional[str] = ..., policy: _Optional[_Union[Policy, _Mapping]] = ..., etag: _Optional[str] = ...) -> None: ...

class UpdatePolicyResponse(_message.Message):
    __slots__ = ("policy",)
    POLICY_FIELD_NUMBER: _ClassVar[int]
    policy: Policy
    def __init__(self, policy: _Optional[_Union[Policy, _Mapping]] = ...) -> None: ...

class GetEffectivePolicyRequest(_message.Message):
    __slots__ = ("parent",)
    PARENT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    def __init__(self, parent: _Optional[str] = ...) -> None: ...

class GetEffectivePolicyResponse(_message.Message):
    __slots__ = ("text", "source", "digest")
    TEXT_FIELD_NUMBER: _ClassVar[int]
    SOURCE_FIELD_NUMBER: _ClassVar[int]
    DIGEST_FIELD_NUMBER: _ClassVar[int]
    text: str
    source: str
    digest: str
    def __init__(self, text: _Optional[str] = ..., source: _Optional[str] = ..., digest: _Optional[str] = ...) -> None: ...

class ExplainLandingRequest(_message.Message):
    __slots__ = ("landing",)
    LANDING_FIELD_NUMBER: _ClassVar[int]
    landing: str
    def __init__(self, landing: _Optional[str] = ...) -> None: ...

class ExplainLandingResponse(_message.Message):
    __slots__ = ("change", "policy", "source", "allowed", "matched_policies", "reasons", "facts_json", "policy_text")
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    POLICY_FIELD_NUMBER: _ClassVar[int]
    SOURCE_FIELD_NUMBER: _ClassVar[int]
    ALLOWED_FIELD_NUMBER: _ClassVar[int]
    MATCHED_POLICIES_FIELD_NUMBER: _ClassVar[int]
    REASONS_FIELD_NUMBER: _ClassVar[int]
    FACTS_JSON_FIELD_NUMBER: _ClassVar[int]
    POLICY_TEXT_FIELD_NUMBER: _ClassVar[int]
    change: str
    policy: str
    source: str
    allowed: bool
    matched_policies: _containers.RepeatedScalarFieldContainer[str]
    reasons: _containers.RepeatedScalarFieldContainer[str]
    facts_json: str
    policy_text: str
    def __init__(self, change: _Optional[str] = ..., policy: _Optional[str] = ..., source: _Optional[str] = ..., allowed: _Optional[bool] = ..., matched_policies: _Optional[_Iterable[str]] = ..., reasons: _Optional[_Iterable[str]] = ..., facts_json: _Optional[str] = ..., policy_text: _Optional[str] = ...) -> None: ...

class SimulatePolicyRequest(_message.Message):
    __slots__ = ("parent", "text", "since", "until")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    TEXT_FIELD_NUMBER: _ClassVar[int]
    SINCE_FIELD_NUMBER: _ClassVar[int]
    UNTIL_FIELD_NUMBER: _ClassVar[int]
    parent: str
    text: str
    since: int
    until: int
    def __init__(self, parent: _Optional[str] = ..., text: _Optional[str] = ..., since: _Optional[int] = ..., until: _Optional[int] = ...) -> None: ...

class SimulatedLanding(_message.Message):
    __slots__ = ("landing", "change", "reasons")
    LANDING_FIELD_NUMBER: _ClassVar[int]
    CHANGE_FIELD_NUMBER: _ClassVar[int]
    REASONS_FIELD_NUMBER: _ClassVar[int]
    landing: str
    change: str
    reasons: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, landing: _Optional[str] = ..., change: _Optional[str] = ..., reasons: _Optional[_Iterable[str]] = ...) -> None: ...

class SimulatePolicyResponse(_message.Message):
    __slots__ = ("replayed", "unrecorded", "allowed", "blocked", "agent_only_before", "agent_only_after")
    REPLAYED_FIELD_NUMBER: _ClassVar[int]
    UNRECORDED_FIELD_NUMBER: _ClassVar[int]
    ALLOWED_FIELD_NUMBER: _ClassVar[int]
    BLOCKED_FIELD_NUMBER: _ClassVar[int]
    AGENT_ONLY_BEFORE_FIELD_NUMBER: _ClassVar[int]
    AGENT_ONLY_AFTER_FIELD_NUMBER: _ClassVar[int]
    replayed: int
    unrecorded: int
    allowed: int
    blocked: _containers.RepeatedCompositeFieldContainer[SimulatedLanding]
    agent_only_before: int
    agent_only_after: int
    def __init__(self, replayed: _Optional[int] = ..., unrecorded: _Optional[int] = ..., allowed: _Optional[int] = ..., blocked: _Optional[_Iterable[_Union[SimulatedLanding, _Mapping]]] = ..., agent_only_before: _Optional[int] = ..., agent_only_after: _Optional[int] = ...) -> None: ...

class GetGuardrailsRequest(_message.Message):
    __slots__ = ("org",)
    ORG_FIELD_NUMBER: _ClassVar[int]
    org: str
    def __init__(self, org: _Optional[str] = ...) -> None: ...

class GetGuardrailsResponse(_message.Message):
    __slots__ = ("text",)
    TEXT_FIELD_NUMBER: _ClassVar[int]
    text: str
    def __init__(self, text: _Optional[str] = ...) -> None: ...

class UpdateGuardrailsRequest(_message.Message):
    __slots__ = ("request_id", "org", "text")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    TEXT_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    org: str
    text: str
    def __init__(self, request_id: _Optional[str] = ..., org: _Optional[str] = ..., text: _Optional[str] = ...) -> None: ...

class UpdateGuardrailsResponse(_message.Message):
    __slots__ = ("text",)
    TEXT_FIELD_NUMBER: _ClassVar[int]
    text: str
    def __init__(self, text: _Optional[str] = ...) -> None: ...
