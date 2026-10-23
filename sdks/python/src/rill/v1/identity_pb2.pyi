from google.protobuf import field_mask_pb2 as _field_mask_pb2
from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class GetCallerRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class Capability(_message.Message):
    __slots__ = ("action", "scope", "expires_at")
    ACTION_FIELD_NUMBER: _ClassVar[int]
    SCOPE_FIELD_NUMBER: _ClassVar[int]
    EXPIRES_AT_FIELD_NUMBER: _ClassVar[int]
    action: str
    scope: str
    expires_at: int
    def __init__(self, action: _Optional[str] = ..., scope: _Optional[str] = ..., expires_at: _Optional[int] = ...) -> None: ...

class GetCallerResponse(_message.Message):
    __slots__ = ("principal", "teams", "capabilities", "token_expires_at", "token_id", "session", "org", "sponsor", "model")
    PRINCIPAL_FIELD_NUMBER: _ClassVar[int]
    TEAMS_FIELD_NUMBER: _ClassVar[int]
    CAPABILITIES_FIELD_NUMBER: _ClassVar[int]
    TOKEN_EXPIRES_AT_FIELD_NUMBER: _ClassVar[int]
    TOKEN_ID_FIELD_NUMBER: _ClassVar[int]
    SESSION_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    SPONSOR_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    principal: _types_pb2.Principal
    teams: _containers.RepeatedScalarFieldContainer[str]
    capabilities: _containers.RepeatedCompositeFieldContainer[Capability]
    token_expires_at: int
    token_id: str
    session: str
    org: str
    sponsor: str
    model: str
    def __init__(self, principal: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., teams: _Optional[_Iterable[str]] = ..., capabilities: _Optional[_Iterable[_Union[Capability, _Mapping]]] = ..., token_expires_at: _Optional[int] = ..., token_id: _Optional[str] = ..., session: _Optional[str] = ..., org: _Optional[str] = ..., sponsor: _Optional[str] = ..., model: _Optional[str] = ...) -> None: ...

class IssueTokenRequest(_message.Message):
    __slots__ = ("request_id", "capabilities", "expires_at", "for_principal")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    CAPABILITIES_FIELD_NUMBER: _ClassVar[int]
    EXPIRES_AT_FIELD_NUMBER: _ClassVar[int]
    FOR_PRINCIPAL_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    capabilities: _containers.RepeatedCompositeFieldContainer[Capability]
    expires_at: int
    for_principal: _types_pb2.Principal
    def __init__(self, request_id: _Optional[str] = ..., capabilities: _Optional[_Iterable[_Union[Capability, _Mapping]]] = ..., expires_at: _Optional[int] = ..., for_principal: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ...) -> None: ...

class IssueTokenResponse(_message.Message):
    __slots__ = ("token", "expires_at")
    TOKEN_FIELD_NUMBER: _ClassVar[int]
    EXPIRES_AT_FIELD_NUMBER: _ClassVar[int]
    token: str
    expires_at: int
    def __init__(self, token: _Optional[str] = ..., expires_at: _Optional[int] = ...) -> None: ...

class ScimCredential(_message.Message):
    __slots__ = ("name", "org", "id", "created_by", "created_at", "revoked_at")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    ID_FIELD_NUMBER: _ClassVar[int]
    CREATED_BY_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    REVOKED_AT_FIELD_NUMBER: _ClassVar[int]
    name: str
    org: str
    id: str
    created_by: _types_pb2.Principal
    created_at: int
    revoked_at: int
    def __init__(self, name: _Optional[str] = ..., org: _Optional[str] = ..., id: _Optional[str] = ..., created_by: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., created_at: _Optional[int] = ..., revoked_at: _Optional[int] = ...) -> None: ...

class CreateScimCredentialRequest(_message.Message):
    __slots__ = ("request_id", "parent")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ...) -> None: ...

class CreateScimCredentialResponse(_message.Message):
    __slots__ = ("credential", "token")
    CREDENTIAL_FIELD_NUMBER: _ClassVar[int]
    TOKEN_FIELD_NUMBER: _ClassVar[int]
    credential: ScimCredential
    token: str
    def __init__(self, credential: _Optional[_Union[ScimCredential, _Mapping]] = ..., token: _Optional[str] = ...) -> None: ...

class ListScimCredentialsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListScimCredentialsResponse(_message.Message):
    __slots__ = ("credentials", "next_page_token")
    CREDENTIALS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    credentials: _containers.RepeatedCompositeFieldContainer[ScimCredential]
    next_page_token: str
    def __init__(self, credentials: _Optional[_Iterable[_Union[ScimCredential, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class DeleteScimCredentialRequest(_message.Message):
    __slots__ = ("request_id", "name")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ...) -> None: ...

class DeleteScimCredentialResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class KillSwitchRequest(_message.Message):
    __slots__ = ("request_id", "parent", "agent", "session", "sponsor", "model", "reason")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    AGENT_FIELD_NUMBER: _ClassVar[int]
    SESSION_FIELD_NUMBER: _ClassVar[int]
    SPONSOR_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    agent: str
    session: str
    sponsor: str
    model: str
    reason: str
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., agent: _Optional[str] = ..., session: _Optional[str] = ..., sponsor: _Optional[str] = ..., model: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class KillSwitchResponse(_message.Message):
    __slots__ = ("agents_suspended", "sessions_revoked", "models_blocked")
    AGENTS_SUSPENDED_FIELD_NUMBER: _ClassVar[int]
    SESSIONS_REVOKED_FIELD_NUMBER: _ClassVar[int]
    MODELS_BLOCKED_FIELD_NUMBER: _ClassVar[int]
    agents_suspended: _containers.RepeatedScalarFieldContainer[str]
    sessions_revoked: _containers.RepeatedScalarFieldContainer[str]
    models_blocked: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, agents_suspended: _Optional[_Iterable[str]] = ..., sessions_revoked: _Optional[_Iterable[str]] = ..., models_blocked: _Optional[_Iterable[str]] = ...) -> None: ...

class AgentBudgets(_message.Message):
    __slots__ = ("versions_per_day", "lands_per_day", "requests_per_minute", "executions_per_day")
    VERSIONS_PER_DAY_FIELD_NUMBER: _ClassVar[int]
    LANDS_PER_DAY_FIELD_NUMBER: _ClassVar[int]
    REQUESTS_PER_MINUTE_FIELD_NUMBER: _ClassVar[int]
    EXECUTIONS_PER_DAY_FIELD_NUMBER: _ClassVar[int]
    versions_per_day: int
    lands_per_day: int
    requests_per_minute: int
    executions_per_day: int
    def __init__(self, versions_per_day: _Optional[int] = ..., lands_per_day: _Optional[int] = ..., requests_per_minute: _Optional[int] = ..., executions_per_day: _Optional[int] = ...) -> None: ...

class WorkloadBinding(_message.Message):
    __slots__ = ("issuer", "subject", "claims")
    class ClaimsEntry(_message.Message):
        __slots__ = ("key", "value")
        KEY_FIELD_NUMBER: _ClassVar[int]
        VALUE_FIELD_NUMBER: _ClassVar[int]
        key: str
        value: str
        def __init__(self, key: _Optional[str] = ..., value: _Optional[str] = ...) -> None: ...
    ISSUER_FIELD_NUMBER: _ClassVar[int]
    SUBJECT_FIELD_NUMBER: _ClassVar[int]
    CLAIMS_FIELD_NUMBER: _ClassVar[int]
    issuer: str
    subject: str
    claims: _containers.ScalarMap[str, str]
    def __init__(self, issuer: _Optional[str] = ..., subject: _Optional[str] = ..., claims: _Optional[_Mapping[str, str]] = ...) -> None: ...

class AgentSpec(_message.Message):
    __slots__ = ("org", "name", "sponsor", "purpose", "repos", "paths", "models", "budgets", "workloads", "expires_at", "reviewer")
    ORG_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    SPONSOR_FIELD_NUMBER: _ClassVar[int]
    PURPOSE_FIELD_NUMBER: _ClassVar[int]
    REPOS_FIELD_NUMBER: _ClassVar[int]
    PATHS_FIELD_NUMBER: _ClassVar[int]
    MODELS_FIELD_NUMBER: _ClassVar[int]
    BUDGETS_FIELD_NUMBER: _ClassVar[int]
    WORKLOADS_FIELD_NUMBER: _ClassVar[int]
    EXPIRES_AT_FIELD_NUMBER: _ClassVar[int]
    REVIEWER_FIELD_NUMBER: _ClassVar[int]
    org: str
    name: str
    sponsor: str
    purpose: str
    repos: _containers.RepeatedScalarFieldContainer[str]
    paths: _containers.RepeatedScalarFieldContainer[str]
    models: _containers.RepeatedScalarFieldContainer[str]
    budgets: AgentBudgets
    workloads: _containers.RepeatedCompositeFieldContainer[WorkloadBinding]
    expires_at: int
    reviewer: ReviewerProfile
    def __init__(self, org: _Optional[str] = ..., name: _Optional[str] = ..., sponsor: _Optional[str] = ..., purpose: _Optional[str] = ..., repos: _Optional[_Iterable[str]] = ..., paths: _Optional[_Iterable[str]] = ..., models: _Optional[_Iterable[str]] = ..., budgets: _Optional[_Union[AgentBudgets, _Mapping]] = ..., workloads: _Optional[_Iterable[_Union[WorkloadBinding, _Mapping]]] = ..., expires_at: _Optional[int] = ..., reviewer: _Optional[_Union[ReviewerProfile, _Mapping]] = ...) -> None: ...

class ReviewerProfile(_message.Message):
    __slots__ = ("model_family", "model", "prompt", "max_vote", "runs_per_day")
    MODEL_FAMILY_FIELD_NUMBER: _ClassVar[int]
    MODEL_FIELD_NUMBER: _ClassVar[int]
    PROMPT_FIELD_NUMBER: _ClassVar[int]
    MAX_VOTE_FIELD_NUMBER: _ClassVar[int]
    RUNS_PER_DAY_FIELD_NUMBER: _ClassVar[int]
    model_family: str
    model: str
    prompt: str
    max_vote: int
    runs_per_day: int
    def __init__(self, model_family: _Optional[str] = ..., model: _Optional[str] = ..., prompt: _Optional[str] = ..., max_vote: _Optional[int] = ..., runs_per_day: _Optional[int] = ...) -> None: ...

class Agent(_message.Message):
    __slots__ = ("name", "etag", "spec", "principal", "status", "created_by", "created_at", "updated_at")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    SPEC_FIELD_NUMBER: _ClassVar[int]
    PRINCIPAL_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    CREATED_BY_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    UPDATED_AT_FIELD_NUMBER: _ClassVar[int]
    name: str
    etag: str
    spec: AgentSpec
    principal: _types_pb2.Principal
    status: str
    created_by: _types_pb2.Principal
    created_at: int
    updated_at: int
    def __init__(self, name: _Optional[str] = ..., etag: _Optional[str] = ..., spec: _Optional[_Union[AgentSpec, _Mapping]] = ..., principal: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., status: _Optional[str] = ..., created_by: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., created_at: _Optional[int] = ..., updated_at: _Optional[int] = ...) -> None: ...

class CreateAgentRequest(_message.Message):
    __slots__ = ("request_id", "parent", "agent")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    AGENT_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    agent: AgentSpec
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., agent: _Optional[_Union[AgentSpec, _Mapping]] = ...) -> None: ...

class CreateAgentResponse(_message.Message):
    __slots__ = ("agent",)
    AGENT_FIELD_NUMBER: _ClassVar[int]
    agent: Agent
    def __init__(self, agent: _Optional[_Union[Agent, _Mapping]] = ...) -> None: ...

class GetAgentRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetAgentResponse(_message.Message):
    __slots__ = ("agent",)
    AGENT_FIELD_NUMBER: _ClassVar[int]
    agent: Agent
    def __init__(self, agent: _Optional[_Union[Agent, _Mapping]] = ...) -> None: ...

class ListAgentsRequest(_message.Message):
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

class ListAgentsResponse(_message.Message):
    __slots__ = ("agents", "next_page_token")
    AGENTS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    agents: _containers.RepeatedCompositeFieldContainer[Agent]
    next_page_token: str
    def __init__(self, agents: _Optional[_Iterable[_Union[Agent, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class UpdateAgentRequest(_message.Message):
    __slots__ = ("request_id", "agent", "update_mask", "etag")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    AGENT_FIELD_NUMBER: _ClassVar[int]
    UPDATE_MASK_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    agent: Agent
    update_mask: _field_mask_pb2.FieldMask
    etag: str
    def __init__(self, request_id: _Optional[str] = ..., agent: _Optional[_Union[Agent, _Mapping]] = ..., update_mask: _Optional[_Union[_field_mask_pb2.FieldMask, _Mapping]] = ..., etag: _Optional[str] = ...) -> None: ...

class UpdateAgentResponse(_message.Message):
    __slots__ = ("agent",)
    AGENT_FIELD_NUMBER: _ClassVar[int]
    agent: Agent
    def __init__(self, agent: _Optional[_Union[Agent, _Mapping]] = ...) -> None: ...

class WorkloadIssuerSpec(_message.Message):
    __slots__ = ("resource_name", "org", "name", "issuer", "discovery", "jwks_uri", "jwks", "audiences")
    RESOURCE_NAME_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    ISSUER_FIELD_NUMBER: _ClassVar[int]
    DISCOVERY_FIELD_NUMBER: _ClassVar[int]
    JWKS_URI_FIELD_NUMBER: _ClassVar[int]
    JWKS_FIELD_NUMBER: _ClassVar[int]
    AUDIENCES_FIELD_NUMBER: _ClassVar[int]
    resource_name: str
    org: str
    name: str
    issuer: str
    discovery: bool
    jwks_uri: str
    jwks: str
    audiences: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, resource_name: _Optional[str] = ..., org: _Optional[str] = ..., name: _Optional[str] = ..., issuer: _Optional[str] = ..., discovery: _Optional[bool] = ..., jwks_uri: _Optional[str] = ..., jwks: _Optional[str] = ..., audiences: _Optional[_Iterable[str]] = ...) -> None: ...

class UpdateWorkloadIssuerRequest(_message.Message):
    __slots__ = ("request_id", "issuer", "allow_missing")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    ISSUER_FIELD_NUMBER: _ClassVar[int]
    ALLOW_MISSING_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    issuer: WorkloadIssuerSpec
    allow_missing: bool
    def __init__(self, request_id: _Optional[str] = ..., issuer: _Optional[_Union[WorkloadIssuerSpec, _Mapping]] = ..., allow_missing: _Optional[bool] = ...) -> None: ...

class UpdateWorkloadIssuerResponse(_message.Message):
    __slots__ = ("issuer",)
    ISSUER_FIELD_NUMBER: _ClassVar[int]
    issuer: WorkloadIssuerSpec
    def __init__(self, issuer: _Optional[_Union[WorkloadIssuerSpec, _Mapping]] = ...) -> None: ...

class ListWorkloadIssuersRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListWorkloadIssuersResponse(_message.Message):
    __slots__ = ("issuers", "next_page_token")
    ISSUERS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    issuers: _containers.RepeatedCompositeFieldContainer[WorkloadIssuerSpec]
    next_page_token: str
    def __init__(self, issuers: _Optional[_Iterable[_Union[WorkloadIssuerSpec, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class DeleteWorkloadIssuerRequest(_message.Message):
    __slots__ = ("request_id", "name")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ...) -> None: ...

class DeleteWorkloadIssuerResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class IdentityProviderSpec(_message.Message):
    __slots__ = ("org", "name", "issuer", "client_id", "client_secret", "cli_client_id", "scopes", "username_claim", "groups_claim", "auto_provision", "saml_metadata", "saml_sp_entity_id")
    ORG_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    ISSUER_FIELD_NUMBER: _ClassVar[int]
    CLIENT_ID_FIELD_NUMBER: _ClassVar[int]
    CLIENT_SECRET_FIELD_NUMBER: _ClassVar[int]
    CLI_CLIENT_ID_FIELD_NUMBER: _ClassVar[int]
    SCOPES_FIELD_NUMBER: _ClassVar[int]
    USERNAME_CLAIM_FIELD_NUMBER: _ClassVar[int]
    GROUPS_CLAIM_FIELD_NUMBER: _ClassVar[int]
    AUTO_PROVISION_FIELD_NUMBER: _ClassVar[int]
    SAML_METADATA_FIELD_NUMBER: _ClassVar[int]
    SAML_SP_ENTITY_ID_FIELD_NUMBER: _ClassVar[int]
    org: str
    name: str
    issuer: str
    client_id: str
    client_secret: str
    cli_client_id: str
    scopes: _containers.RepeatedScalarFieldContainer[str]
    username_claim: str
    groups_claim: str
    auto_provision: bool
    saml_metadata: str
    saml_sp_entity_id: str
    def __init__(self, org: _Optional[str] = ..., name: _Optional[str] = ..., issuer: _Optional[str] = ..., client_id: _Optional[str] = ..., client_secret: _Optional[str] = ..., cli_client_id: _Optional[str] = ..., scopes: _Optional[_Iterable[str]] = ..., username_claim: _Optional[str] = ..., groups_claim: _Optional[str] = ..., auto_provision: _Optional[bool] = ..., saml_metadata: _Optional[str] = ..., saml_sp_entity_id: _Optional[str] = ...) -> None: ...

class IdentityProviderInfo(_message.Message):
    __slots__ = ("resource_name", "org", "name", "issuer", "client_id", "has_client_secret", "cli_client_id", "scopes", "username_claim", "groups_claim", "auto_provision", "created_at", "saml", "saml_sp_entity_id")
    RESOURCE_NAME_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    ISSUER_FIELD_NUMBER: _ClassVar[int]
    CLIENT_ID_FIELD_NUMBER: _ClassVar[int]
    HAS_CLIENT_SECRET_FIELD_NUMBER: _ClassVar[int]
    CLI_CLIENT_ID_FIELD_NUMBER: _ClassVar[int]
    SCOPES_FIELD_NUMBER: _ClassVar[int]
    USERNAME_CLAIM_FIELD_NUMBER: _ClassVar[int]
    GROUPS_CLAIM_FIELD_NUMBER: _ClassVar[int]
    AUTO_PROVISION_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    SAML_FIELD_NUMBER: _ClassVar[int]
    SAML_SP_ENTITY_ID_FIELD_NUMBER: _ClassVar[int]
    resource_name: str
    org: str
    name: str
    issuer: str
    client_id: str
    has_client_secret: bool
    cli_client_id: str
    scopes: _containers.RepeatedScalarFieldContainer[str]
    username_claim: str
    groups_claim: str
    auto_provision: bool
    created_at: int
    saml: bool
    saml_sp_entity_id: str
    def __init__(self, resource_name: _Optional[str] = ..., org: _Optional[str] = ..., name: _Optional[str] = ..., issuer: _Optional[str] = ..., client_id: _Optional[str] = ..., has_client_secret: _Optional[bool] = ..., cli_client_id: _Optional[str] = ..., scopes: _Optional[_Iterable[str]] = ..., username_claim: _Optional[str] = ..., groups_claim: _Optional[str] = ..., auto_provision: _Optional[bool] = ..., created_at: _Optional[int] = ..., saml: _Optional[bool] = ..., saml_sp_entity_id: _Optional[str] = ...) -> None: ...

class GetIdentityProviderRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetIdentityProviderResponse(_message.Message):
    __slots__ = ("provider",)
    PROVIDER_FIELD_NUMBER: _ClassVar[int]
    provider: IdentityProviderInfo
    def __init__(self, provider: _Optional[_Union[IdentityProviderInfo, _Mapping]] = ...) -> None: ...

class UpdateIdentityProviderRequest(_message.Message):
    __slots__ = ("request_id", "provider", "allow_missing")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PROVIDER_FIELD_NUMBER: _ClassVar[int]
    ALLOW_MISSING_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    provider: IdentityProviderSpec
    allow_missing: bool
    def __init__(self, request_id: _Optional[str] = ..., provider: _Optional[_Union[IdentityProviderSpec, _Mapping]] = ..., allow_missing: _Optional[bool] = ...) -> None: ...

class UpdateIdentityProviderResponse(_message.Message):
    __slots__ = ("provider",)
    PROVIDER_FIELD_NUMBER: _ClassVar[int]
    provider: IdentityProviderInfo
    def __init__(self, provider: _Optional[_Union[IdentityProviderInfo, _Mapping]] = ...) -> None: ...

class ListIdentityProvidersRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListIdentityProvidersResponse(_message.Message):
    __slots__ = ("providers", "next_page_token")
    PROVIDERS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    providers: _containers.RepeatedCompositeFieldContainer[IdentityProviderInfo]
    next_page_token: str
    def __init__(self, providers: _Optional[_Iterable[_Union[IdentityProviderInfo, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class DeleteIdentityProviderRequest(_message.Message):
    __slots__ = ("request_id", "name")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ...) -> None: ...

class DeleteIdentityProviderResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class User(_message.Message):
    __slots__ = ("name", "etag", "org", "principal", "email", "display_name", "status", "roles", "teams", "external", "created_at", "updated_at")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    ORG_FIELD_NUMBER: _ClassVar[int]
    PRINCIPAL_FIELD_NUMBER: _ClassVar[int]
    EMAIL_FIELD_NUMBER: _ClassVar[int]
    DISPLAY_NAME_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    ROLES_FIELD_NUMBER: _ClassVar[int]
    TEAMS_FIELD_NUMBER: _ClassVar[int]
    EXTERNAL_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    UPDATED_AT_FIELD_NUMBER: _ClassVar[int]
    name: str
    etag: str
    org: str
    principal: _types_pb2.Principal
    email: str
    display_name: str
    status: str
    roles: _containers.RepeatedScalarFieldContainer[str]
    teams: _containers.RepeatedScalarFieldContainer[str]
    external: _containers.RepeatedScalarFieldContainer[str]
    created_at: int
    updated_at: int
    def __init__(self, name: _Optional[str] = ..., etag: _Optional[str] = ..., org: _Optional[str] = ..., principal: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., email: _Optional[str] = ..., display_name: _Optional[str] = ..., status: _Optional[str] = ..., roles: _Optional[_Iterable[str]] = ..., teams: _Optional[_Iterable[str]] = ..., external: _Optional[_Iterable[str]] = ..., created_at: _Optional[int] = ..., updated_at: _Optional[int] = ...) -> None: ...

class ListUsersRequest(_message.Message):
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

class ListUsersResponse(_message.Message):
    __slots__ = ("users", "next_page_token")
    USERS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    users: _containers.RepeatedCompositeFieldContainer[User]
    next_page_token: str
    def __init__(self, users: _Optional[_Iterable[_Union[User, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class UpdateUserRequest(_message.Message):
    __slots__ = ("request_id", "user", "update_mask", "etag")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    USER_FIELD_NUMBER: _ClassVar[int]
    UPDATE_MASK_FIELD_NUMBER: _ClassVar[int]
    ETAG_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    user: User
    update_mask: _field_mask_pb2.FieldMask
    etag: str
    def __init__(self, request_id: _Optional[str] = ..., user: _Optional[_Union[User, _Mapping]] = ..., update_mask: _Optional[_Union[_field_mask_pb2.FieldMask, _Mapping]] = ..., etag: _Optional[str] = ...) -> None: ...

class UpdateUserResponse(_message.Message):
    __slots__ = ("user",)
    USER_FIELD_NUMBER: _ClassVar[int]
    user: User
    def __init__(self, user: _Optional[_Union[User, _Mapping]] = ...) -> None: ...

class RevokeTokenRequest(_message.Message):
    __slots__ = ("request_id", "token", "token_id", "reason")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    TOKEN_FIELD_NUMBER: _ClassVar[int]
    TOKEN_ID_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    token: str
    token_id: str
    reason: str
    def __init__(self, request_id: _Optional[str] = ..., token: _Optional[str] = ..., token_id: _Optional[str] = ..., reason: _Optional[str] = ...) -> None: ...

class RevokeTokenResponse(_message.Message):
    __slots__ = ("token_id",)
    TOKEN_ID_FIELD_NUMBER: _ClassVar[int]
    token_id: str
    def __init__(self, token_id: _Optional[str] = ...) -> None: ...

class RevokeSessionRequest(_message.Message):
    __slots__ = ("request_id", "session")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    SESSION_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    session: str
    def __init__(self, request_id: _Optional[str] = ..., session: _Optional[str] = ...) -> None: ...

class RevokeSessionResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class RotateTokenKeysRequest(_message.Message):
    __slots__ = ("request_id",)
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    def __init__(self, request_id: _Optional[str] = ...) -> None: ...

class RotateTokenKeysResponse(_message.Message):
    __slots__ = ("key_id", "active_from")
    KEY_ID_FIELD_NUMBER: _ClassVar[int]
    ACTIVE_FROM_FIELD_NUMBER: _ClassVar[int]
    key_id: int
    active_from: int
    def __init__(self, key_id: _Optional[int] = ..., active_from: _Optional[int] = ...) -> None: ...

class ListTokenKeysRequest(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class TokenKey(_message.Message):
    __slots__ = ("id", "public_key", "backend", "key_ref", "created_at", "active_from", "not_after")
    ID_FIELD_NUMBER: _ClassVar[int]
    PUBLIC_KEY_FIELD_NUMBER: _ClassVar[int]
    BACKEND_FIELD_NUMBER: _ClassVar[int]
    KEY_REF_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    ACTIVE_FROM_FIELD_NUMBER: _ClassVar[int]
    NOT_AFTER_FIELD_NUMBER: _ClassVar[int]
    id: int
    public_key: str
    backend: str
    key_ref: str
    created_at: int
    active_from: int
    not_after: int
    def __init__(self, id: _Optional[int] = ..., public_key: _Optional[str] = ..., backend: _Optional[str] = ..., key_ref: _Optional[str] = ..., created_at: _Optional[int] = ..., active_from: _Optional[int] = ..., not_after: _Optional[int] = ...) -> None: ...

class ListTokenKeysResponse(_message.Message):
    __slots__ = ("keys",)
    KEYS_FIELD_NUMBER: _ClassVar[int]
    keys: _containers.RepeatedCompositeFieldContainer[TokenKey]
    def __init__(self, keys: _Optional[_Iterable[_Union[TokenKey, _Mapping]]] = ...) -> None: ...
