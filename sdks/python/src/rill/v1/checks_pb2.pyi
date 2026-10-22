from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf.internal import enum_type_wrapper as _enum_type_wrapper
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class StepState(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    STEP_STATE_UNSPECIFIED: _ClassVar[StepState]
    STEP_STATE_QUEUED: _ClassVar[StepState]
    STEP_STATE_RUNNING: _ClassVar[StepState]
    STEP_STATE_PASSED: _ClassVar[StepState]
    STEP_STATE_FAILED: _ClassVar[StepState]
    STEP_STATE_TIMED_OUT: _ClassVar[StepState]
    STEP_STATE_ERROR: _ClassVar[StepState]
    STEP_STATE_UNAFFECTED: _ClassVar[StepState]
    STEP_STATE_NOT_GATING: _ClassVar[StepState]

class LogStream(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    LOG_STREAM_UNSPECIFIED: _ClassVar[LogStream]
    LOG_STREAM_STDOUT: _ClassVar[LogStream]
    LOG_STREAM_STDERR: _ClassVar[LogStream]

class TestOutcome(int, metaclass=_enum_type_wrapper.EnumTypeWrapper):
    __slots__ = ()
    TEST_OUTCOME_UNSPECIFIED: _ClassVar[TestOutcome]
    TEST_OUTCOME_PASSED: _ClassVar[TestOutcome]
    TEST_OUTCOME_FAILED: _ClassVar[TestOutcome]
    TEST_OUTCOME_SKIPPED: _ClassVar[TestOutcome]
    TEST_OUTCOME_ERROR: _ClassVar[TestOutcome]
STEP_STATE_UNSPECIFIED: StepState
STEP_STATE_QUEUED: StepState
STEP_STATE_RUNNING: StepState
STEP_STATE_PASSED: StepState
STEP_STATE_FAILED: StepState
STEP_STATE_TIMED_OUT: StepState
STEP_STATE_ERROR: StepState
STEP_STATE_UNAFFECTED: StepState
STEP_STATE_NOT_GATING: StepState
LOG_STREAM_UNSPECIFIED: LogStream
LOG_STREAM_STDOUT: LogStream
LOG_STREAM_STDERR: LogStream
TEST_OUTCOME_UNSPECIFIED: TestOutcome
TEST_OUTCOME_PASSED: TestOutcome
TEST_OUTCOME_FAILED: TestOutcome
TEST_OUTCOME_SKIPPED: TestOutcome
TEST_OUTCOME_ERROR: TestOutcome

class Quarantine(_message.Message):
    __slots__ = ("name", "target", "since", "by", "reason", "intent")
    NAME_FIELD_NUMBER: _ClassVar[int]
    TARGET_FIELD_NUMBER: _ClassVar[int]
    SINCE_FIELD_NUMBER: _ClassVar[int]
    BY_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    INTENT_FIELD_NUMBER: _ClassVar[int]
    name: str
    target: str
    since: int
    by: _types_pb2.Principal
    reason: str
    intent: bytes
    def __init__(self, name: _Optional[str] = ..., target: _Optional[str] = ..., since: _Optional[int] = ..., by: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., reason: _Optional[str] = ..., intent: _Optional[bytes] = ...) -> None: ...

class ListQuarantinesRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListQuarantinesResponse(_message.Message):
    __slots__ = ("quarantines", "next_page_token")
    QUARANTINES_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    quarantines: _containers.RepeatedCompositeFieldContainer[Quarantine]
    next_page_token: str
    def __init__(self, quarantines: _Optional[_Iterable[_Union[Quarantine, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class CreateQuarantineRequest(_message.Message):
    __slots__ = ("request_id", "parent", "quarantine")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    QUARANTINE_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    quarantine: Quarantine
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., quarantine: _Optional[_Union[Quarantine, _Mapping]] = ...) -> None: ...

class CreateQuarantineResponse(_message.Message):
    __slots__ = ("quarantine",)
    QUARANTINE_FIELD_NUMBER: _ClassVar[int]
    quarantine: Quarantine
    def __init__(self, quarantine: _Optional[_Union[Quarantine, _Mapping]] = ...) -> None: ...

class DeleteQuarantineRequest(_message.Message):
    __slots__ = ("request_id", "name")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ...) -> None: ...

class DeleteQuarantineResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class NonhermeticStep(_message.Message):
    __slots__ = ("step", "action", "differences", "found_at", "until")
    STEP_FIELD_NUMBER: _ClassVar[int]
    ACTION_FIELD_NUMBER: _ClassVar[int]
    DIFFERENCES_FIELD_NUMBER: _ClassVar[int]
    FOUND_AT_FIELD_NUMBER: _ClassVar[int]
    UNTIL_FIELD_NUMBER: _ClassVar[int]
    step: str
    action: bytes
    differences: _containers.RepeatedScalarFieldContainer[str]
    found_at: int
    until: int
    def __init__(self, step: _Optional[str] = ..., action: _Optional[bytes] = ..., differences: _Optional[_Iterable[str]] = ..., found_at: _Optional[int] = ..., until: _Optional[int] = ...) -> None: ...

class ListNonhermeticStepsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ...) -> None: ...

class ListNonhermeticStepsResponse(_message.Message):
    __slots__ = ("steps", "next_page_token")
    STEPS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    steps: _containers.RepeatedCompositeFieldContainer[NonhermeticStep]
    next_page_token: str
    def __init__(self, steps: _Optional[_Iterable[_Union[NonhermeticStep, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class Artifact(_message.Message):
    __slots__ = ("path", "size")
    PATH_FIELD_NUMBER: _ClassVar[int]
    SIZE_FIELD_NUMBER: _ClassVar[int]
    path: str
    size: int
    def __init__(self, path: _Optional[str] = ..., size: _Optional[int] = ...) -> None: ...

class RunStep(_message.Message):
    __slots__ = ("index", "name", "shard", "shards", "state", "exit_code", "error", "cached", "stdout_size", "stderr_size", "artifacts", "worker", "duration_ms", "nonhermetic")
    INDEX_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    SHARD_FIELD_NUMBER: _ClassVar[int]
    SHARDS_FIELD_NUMBER: _ClassVar[int]
    STATE_FIELD_NUMBER: _ClassVar[int]
    EXIT_CODE_FIELD_NUMBER: _ClassVar[int]
    ERROR_FIELD_NUMBER: _ClassVar[int]
    CACHED_FIELD_NUMBER: _ClassVar[int]
    STDOUT_SIZE_FIELD_NUMBER: _ClassVar[int]
    STDERR_SIZE_FIELD_NUMBER: _ClassVar[int]
    ARTIFACTS_FIELD_NUMBER: _ClassVar[int]
    WORKER_FIELD_NUMBER: _ClassVar[int]
    DURATION_MS_FIELD_NUMBER: _ClassVar[int]
    NONHERMETIC_FIELD_NUMBER: _ClassVar[int]
    index: int
    name: str
    shard: int
    shards: int
    state: StepState
    exit_code: int
    error: str
    cached: bool
    stdout_size: int
    stderr_size: int
    artifacts: _containers.RepeatedCompositeFieldContainer[Artifact]
    worker: str
    duration_ms: int
    nonhermetic: bool
    def __init__(self, index: _Optional[int] = ..., name: _Optional[str] = ..., shard: _Optional[int] = ..., shards: _Optional[int] = ..., state: _Optional[_Union[StepState, str]] = ..., exit_code: _Optional[int] = ..., error: _Optional[str] = ..., cached: _Optional[bool] = ..., stdout_size: _Optional[int] = ..., stderr_size: _Optional[int] = ..., artifacts: _Optional[_Iterable[_Union[Artifact, _Mapping]]] = ..., worker: _Optional[str] = ..., duration_ms: _Optional[int] = ..., nonhermetic: _Optional[bool] = ...) -> None: ...

class VerifyRun(_message.Message):
    __slots__ = ("name", "id", "version", "trunk", "started_at", "green", "summary", "finished_at", "steps")
    NAME_FIELD_NUMBER: _ClassVar[int]
    ID_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    TRUNK_FIELD_NUMBER: _ClassVar[int]
    STARTED_AT_FIELD_NUMBER: _ClassVar[int]
    GREEN_FIELD_NUMBER: _ClassVar[int]
    SUMMARY_FIELD_NUMBER: _ClassVar[int]
    FINISHED_AT_FIELD_NUMBER: _ClassVar[int]
    STEPS_FIELD_NUMBER: _ClassVar[int]
    name: str
    id: str
    version: bytes
    trunk: bool
    started_at: int
    green: bool
    summary: str
    finished_at: int
    steps: _containers.RepeatedCompositeFieldContainer[RunStep]
    def __init__(self, name: _Optional[str] = ..., id: _Optional[str] = ..., version: _Optional[bytes] = ..., trunk: _Optional[bool] = ..., started_at: _Optional[int] = ..., green: _Optional[bool] = ..., summary: _Optional[str] = ..., finished_at: _Optional[int] = ..., steps: _Optional[_Iterable[_Union[RunStep, _Mapping]]] = ...) -> None: ...

class ListRunsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token", "version")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    version: str
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ..., version: _Optional[str] = ...) -> None: ...

class ListRunsResponse(_message.Message):
    __slots__ = ("runs", "next_page_token")
    RUNS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    runs: _containers.RepeatedCompositeFieldContainer[VerifyRun]
    next_page_token: str
    def __init__(self, runs: _Optional[_Iterable[_Union[VerifyRun, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...

class GetRunRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class GetRunResponse(_message.Message):
    __slots__ = ("run",)
    RUN_FIELD_NUMBER: _ClassVar[int]
    run: VerifyRun
    def __init__(self, run: _Optional[_Union[VerifyRun, _Mapping]] = ...) -> None: ...

class WatchRunRequest(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class WatchRunResponse(_message.Message):
    __slots__ = ("run",)
    RUN_FIELD_NUMBER: _ClassVar[int]
    run: VerifyRun
    def __init__(self, run: _Optional[_Union[VerifyRun, _Mapping]] = ...) -> None: ...

class ReadLogRequest(_message.Message):
    __slots__ = ("run", "step", "stream", "offset", "limit")
    RUN_FIELD_NUMBER: _ClassVar[int]
    STEP_FIELD_NUMBER: _ClassVar[int]
    STREAM_FIELD_NUMBER: _ClassVar[int]
    OFFSET_FIELD_NUMBER: _ClassVar[int]
    LIMIT_FIELD_NUMBER: _ClassVar[int]
    run: str
    step: int
    stream: LogStream
    offset: int
    limit: int
    def __init__(self, run: _Optional[str] = ..., step: _Optional[int] = ..., stream: _Optional[_Union[LogStream, str]] = ..., offset: _Optional[int] = ..., limit: _Optional[int] = ...) -> None: ...

class ReadLogResponse(_message.Message):
    __slots__ = ("data", "offset", "size")
    DATA_FIELD_NUMBER: _ClassVar[int]
    OFFSET_FIELD_NUMBER: _ClassVar[int]
    SIZE_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    offset: int
    size: int
    def __init__(self, data: _Optional[bytes] = ..., offset: _Optional[int] = ..., size: _Optional[int] = ...) -> None: ...

class ReadArtifactRequest(_message.Message):
    __slots__ = ("run", "step", "path", "offset", "limit")
    RUN_FIELD_NUMBER: _ClassVar[int]
    STEP_FIELD_NUMBER: _ClassVar[int]
    PATH_FIELD_NUMBER: _ClassVar[int]
    OFFSET_FIELD_NUMBER: _ClassVar[int]
    LIMIT_FIELD_NUMBER: _ClassVar[int]
    run: str
    step: int
    path: str
    offset: int
    limit: int
    def __init__(self, run: _Optional[str] = ..., step: _Optional[int] = ..., path: _Optional[str] = ..., offset: _Optional[int] = ..., limit: _Optional[int] = ...) -> None: ...

class ReadArtifactResponse(_message.Message):
    __slots__ = ("data", "offset", "size")
    DATA_FIELD_NUMBER: _ClassVar[int]
    OFFSET_FIELD_NUMBER: _ClassVar[int]
    SIZE_FIELD_NUMBER: _ClassVar[int]
    data: bytes
    offset: int
    size: int
    def __init__(self, data: _Optional[bytes] = ..., offset: _Optional[int] = ..., size: _Optional[int] = ...) -> None: ...

class TestRun(_message.Message):
    __slots__ = ("target", "outcome", "retried", "duration_ms", "action", "worker", "version", "trunk", "at")
    TARGET_FIELD_NUMBER: _ClassVar[int]
    OUTCOME_FIELD_NUMBER: _ClassVar[int]
    RETRIED_FIELD_NUMBER: _ClassVar[int]
    DURATION_MS_FIELD_NUMBER: _ClassVar[int]
    ACTION_FIELD_NUMBER: _ClassVar[int]
    WORKER_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    TRUNK_FIELD_NUMBER: _ClassVar[int]
    AT_FIELD_NUMBER: _ClassVar[int]
    target: str
    outcome: TestOutcome
    retried: bool
    duration_ms: int
    action: bytes
    worker: str
    version: bytes
    trunk: bool
    at: int
    def __init__(self, target: _Optional[str] = ..., outcome: _Optional[_Union[TestOutcome, str]] = ..., retried: _Optional[bool] = ..., duration_ms: _Optional[int] = ..., action: _Optional[bytes] = ..., worker: _Optional[str] = ..., version: _Optional[bytes] = ..., trunk: _Optional[bool] = ..., at: _Optional[int] = ...) -> None: ...

class GetTestHistoryRequest(_message.Message):
    __slots__ = ("parent", "target", "max_results")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    TARGET_FIELD_NUMBER: _ClassVar[int]
    MAX_RESULTS_FIELD_NUMBER: _ClassVar[int]
    parent: str
    target: str
    max_results: int
    def __init__(self, parent: _Optional[str] = ..., target: _Optional[str] = ..., max_results: _Optional[int] = ...) -> None: ...

class GetTestHistoryResponse(_message.Message):
    __slots__ = ("runs", "stats")
    RUNS_FIELD_NUMBER: _ClassVar[int]
    STATS_FIELD_NUMBER: _ClassVar[int]
    runs: _containers.RepeatedCompositeFieldContainer[TestRun]
    stats: FlakyTest
    def __init__(self, runs: _Optional[_Iterable[_Union[TestRun, _Mapping]]] = ..., stats: _Optional[_Union[FlakyTest, _Mapping]] = ...) -> None: ...

class FlakyTest(_message.Message):
    __slots__ = ("target", "score", "runs", "failures", "flips", "last")
    TARGET_FIELD_NUMBER: _ClassVar[int]
    SCORE_FIELD_NUMBER: _ClassVar[int]
    RUNS_FIELD_NUMBER: _ClassVar[int]
    FAILURES_FIELD_NUMBER: _ClassVar[int]
    FLIPS_FIELD_NUMBER: _ClassVar[int]
    LAST_FIELD_NUMBER: _ClassVar[int]
    target: str
    score: float
    runs: float
    failures: float
    flips: float
    last: TestOutcome
    def __init__(self, target: _Optional[str] = ..., score: _Optional[float] = ..., runs: _Optional[float] = ..., failures: _Optional[float] = ..., flips: _Optional[float] = ..., last: _Optional[_Union[TestOutcome, str]] = ...) -> None: ...

class ListFlakyTestsRequest(_message.Message):
    __slots__ = ("parent", "page_size", "page_token", "min_score")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PAGE_SIZE_FIELD_NUMBER: _ClassVar[int]
    PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    MIN_SCORE_FIELD_NUMBER: _ClassVar[int]
    parent: str
    page_size: int
    page_token: str
    min_score: float
    def __init__(self, parent: _Optional[str] = ..., page_size: _Optional[int] = ..., page_token: _Optional[str] = ..., min_score: _Optional[float] = ...) -> None: ...

class ListFlakyTestsResponse(_message.Message):
    __slots__ = ("tests", "next_page_token")
    TESTS_FIELD_NUMBER: _ClassVar[int]
    NEXT_PAGE_TOKEN_FIELD_NUMBER: _ClassVar[int]
    tests: _containers.RepeatedCompositeFieldContainer[FlakyTest]
    next_page_token: str
    def __init__(self, tests: _Optional[_Iterable[_Union[FlakyTest, _Mapping]]] = ..., next_page_token: _Optional[str] = ...) -> None: ...
