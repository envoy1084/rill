"""Retrying calls that may safely run again."""

from __future__ import annotations

import random
import time
from typing import Callable, TypeVar

from connectrpc.code import Code
from connectrpc.errors import ConnectError

from .errors import retry_after

T = TypeVar("T")

RETRY_BUDGET = 20.0
"""Seconds ``retry`` keeps trying: past a dead server's idempotency claim."""


def retry(call: Callable[[], T]) -> T:
    """Run ``call`` until it succeeds, fails for good, or the budget runs out.

    Retries while the cluster is unavailable and while the server refuses
    for now with a wait (a rate limit, a full queue). Safe for reads and
    for mutations carrying a ``request_id``, which the server runs at most
    once.
    """
    deadline = time.monotonic() + RETRY_BUDGET
    delay = 0.1
    while True:
        try:
            return call()
        except ConnectError as err:
            wait = None
            if err.code == Code.UNAVAILABLE:
                wait = delay / 2 + random.random() * delay / 2
                delay = min(delay * 2, 2.0)
            elif err.code == Code.RESOURCE_EXHAUSTED:
                wait = retry_after(err)
            if wait is None:
                raise
            wait = min(wait, 5.0)
            if time.monotonic() + wait > deadline:
                raise
            time.sleep(wait)
