"""What an error's details say: the reason, what a policy denial misses,
and how long to wait."""

from __future__ import annotations

from connectrpc.errors import ConnectError
from google.rpc import error_details_pb2

ERROR_DOMAIN = "rill.dev"
"""The ErrorInfo domain of Rill's error reasons."""


def _details(err: BaseException, kind):
    if not isinstance(err, ConnectError):
        return []
    out = []
    for any_detail in err.details:
        message = kind()
        if any_detail.Is(message.DESCRIPTOR) and any_detail.Unpack(message):
            out.append(message)
    return out


def reason(err: BaseException) -> str | None:
    """The error's reason (``NOT_FOUND``, ``ETAG_MISMATCH``, …), or None."""
    for info in _details(err, error_details_pb2.ErrorInfo):
        if info.domain == ERROR_DOMAIN:
            return info.reason
    return None


def is_reason(err: BaseException, why: str) -> bool:
    """Whether ``err`` is a refusal for ``why``."""
    return reason(err) == why


def missing(err: BaseException) -> list[str]:
    """What a policy denial still needs."""
    return [
        v.description
        for failure in _details(err, error_details_pb2.PreconditionFailure)
        for v in failure.violations
    ]


def retry_after(err: BaseException) -> float | None:
    """How long the server asked the caller to wait, in seconds."""
    for info in _details(err, error_details_pb2.RetryInfo):
        return info.retry_delay.seconds + info.retry_delay.nanos / 1e9
    return None
