"""The rill.v1 API for Python: generated Connect clients and messages
(package ``rill.v1``), one client holding them all, names, error reasons
and retries."""

from . import names
from .client import RillClient, connect
from .errors import ERROR_DOMAIN, is_reason, missing, reason, retry_after
from .retry import RETRY_BUDGET, retry

__all__ = [
    "ERROR_DOMAIN",
    "RETRY_BUDGET",
    "RillClient",
    "connect",
    "is_reason",
    "missing",
    "names",
    "reason",
    "retry",
    "retry_after",
]
