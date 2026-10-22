"""A client holding every rill.v1 service, with an interceptor that
authenticates and identifies each call."""

from __future__ import annotations

from dataclasses import dataclass

from rill.v1.access_connect import AccessServiceClientSync
from rill.v1.admin_connect import AdminServiceClientSync
from rill.v1.audit_connect import AuditServiceClientSync
from rill.v1.changes_connect import ChangeServiceClientSync
from rill.v1.checks_connect import ChecksServiceClientSync
from rill.v1.content_connect import ContentServiceClientSync
from rill.v1.events_connect import EventServiceClientSync
from rill.v1.identity_connect import IdentityServiceClientSync
from rill.v1.intent_connect import IntentServiceClientSync
from rill.v1.landing_connect import LandingServiceClientSync
from rill.v1.operations_connect import OperationServiceClientSync
from rill.v1.orgs_connect import OrgServiceClientSync
from rill.v1.policy_connect import PolicyServiceClientSync
from rill.v1.protection_connect import ProtectionServiceClientSync
from rill.v1.repos_connect import RepoServiceClientSync
from rill.v1.review_connect import ReviewServiceClientSync
from rill.v1.secrets_connect import SecretServiceClientSync
from rill.v1.session_connect import SessionServiceClientSync
from rill.v1.trunk_connect import TrunkServiceClientSync
from rill.v1.workspace_connect import WorkspaceServiceClientSync

from .names import request_id


class _Headers:
    """Sets the credential, a fresh x-request-id and the user agent."""

    def __init__(self, token: str | None, user_agent: str) -> None:
        self._token = token
        self._user_agent = user_agent

    def _set(self, ctx) -> None:
        headers = ctx.request_headers()
        if self._token:
            headers["authorization"] = f"Bearer {self._token}"
        headers["x-request-id"] = request_id()
        headers["user-agent"] = self._user_agent

    def intercept_unary_sync(self, call_next, request, ctx):
        self._set(ctx)
        return call_next(request, ctx)

    def intercept_client_stream_sync(self, call_next, request, ctx):
        self._set(ctx)
        return call_next(request, ctx)

    def intercept_server_stream_sync(self, call_next, request, ctx):
        self._set(ctx)
        return call_next(request, ctx)

    def intercept_bidi_stream_sync(self, call_next, request, ctx):
        self._set(ctx)
        return call_next(request, ctx)


@dataclass
class RillClient:
    """A generated client for every rill.v1 service."""

    repos: RepoServiceClientSync
    content: ContentServiceClientSync
    changes: ChangeServiceClientSync
    events: EventServiceClientSync
    workspaces: WorkspaceServiceClientSync
    reviews: ReviewServiceClientSync
    landing: LandingServiceClientSync
    trunk: TrunkServiceClientSync
    operations: OperationServiceClientSync
    intents: IntentServiceClientSync
    checks: ChecksServiceClientSync
    policies: PolicyServiceClientSync
    orgs: OrgServiceClientSync
    admin: AdminServiceClientSync
    access: AccessServiceClientSync
    identity: IdentityServiceClientSync
    sessions: SessionServiceClientSync
    audit: AuditServiceClientSync
    protection: ProtectionServiceClientSync
    secrets: SecretServiceClientSync


def connect(endpoint: str, *, token: str | None = None, user_agent: str = "rill-sdk-python") -> RillClient:
    """A client for the server at ``endpoint`` (``http://host:port``)."""
    interceptors = (_Headers(token, user_agent),)

    def make(cls):
        return cls(endpoint, interceptors=interceptors)

    return RillClient(
        repos=make(RepoServiceClientSync),
        content=make(ContentServiceClientSync),
        changes=make(ChangeServiceClientSync),
        events=make(EventServiceClientSync),
        workspaces=make(WorkspaceServiceClientSync),
        reviews=make(ReviewServiceClientSync),
        landing=make(LandingServiceClientSync),
        trunk=make(TrunkServiceClientSync),
        operations=make(OperationServiceClientSync),
        intents=make(IntentServiceClientSync),
        checks=make(ChecksServiceClientSync),
        policies=make(PolicyServiceClientSync),
        orgs=make(OrgServiceClientSync),
        admin=make(AdminServiceClientSync),
        access=make(AccessServiceClientSync),
        identity=make(IdentityServiceClientSync),
        sessions=make(SessionServiceClientSync),
        audit=make(AuditServiceClientSync),
        protection=make(ProtectionServiceClientSync),
        secrets=make(SecretServiceClientSync),
    )
