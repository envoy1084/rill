"""Resource names, built from what callers hold."""

from __future__ import annotations

import uuid

DEFAULT_ORG = "default"
"""The org a bare repository name is in."""


def repo(spec: str) -> str:
    """``orgs/{org}/repos/{repo}`` for a bare name, ``org/name`` or a full name."""
    if spec.startswith("orgs/"):
        return spec
    org_name, slash, name = spec.partition("/")
    return f"orgs/{org_name}/repos/{name}" if slash else f"orgs/{DEFAULT_ORG}/repos/{spec}"


def org(name: str = "") -> str:
    """``orgs/{org}``; the default org when ``name`` is empty."""
    return f"orgs/{name or DEFAULT_ORG}"


def change(repo_spec: str, change_id: bytes) -> str:
    """``{repo}/changes/ch_{hex}`` for a change id as messages carry it."""
    return f"{repo(repo_spec)}/changes/ch_{change_id.hex()}"


def review(repo_spec: str, change_id: bytes) -> str:
    """The review of a change."""
    return f"{change(repo_spec, change_id)}/review"


def version(repo_spec: str, version_hash: bytes) -> str:
    """``{repo}/versions/{hex}`` for a version hash."""
    return f"{repo(repo_spec)}/versions/{version_hash.hex()}"


def trunk_position(repo_spec: str, n: int | str) -> str:
    """``{repo}/trunk/positions/{n}``, or ``head``."""
    return f"{repo(repo_spec)}/trunk/positions/{n}"


def policy(repo_spec: str, name: str) -> str:
    """``{repo}/policies/{name}``."""
    return f"{repo(repo_spec)}/policies/{name}"


def request_id() -> str:
    """A fresh idempotency key."""
    return str(uuid.uuid4())
