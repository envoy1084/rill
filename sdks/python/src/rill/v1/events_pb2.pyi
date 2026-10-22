from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class WatchEventsRequest(_message.Message):
    __slots__ = ("parent", "after", "kinds", "change_id", "ref_prefix")
    PARENT_FIELD_NUMBER: _ClassVar[int]
    AFTER_FIELD_NUMBER: _ClassVar[int]
    KINDS_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    REF_PREFIX_FIELD_NUMBER: _ClassVar[int]
    parent: str
    after: bytes
    kinds: _containers.RepeatedScalarFieldContainer[str]
    change_id: bytes
    ref_prefix: str
    def __init__(self, parent: _Optional[str] = ..., after: _Optional[bytes] = ..., kinds: _Optional[_Iterable[str]] = ..., change_id: _Optional[bytes] = ..., ref_prefix: _Optional[str] = ...) -> None: ...

class WatchEventsResponse(_message.Message):
    __slots__ = ("cursor", "event")
    CURSOR_FIELD_NUMBER: _ClassVar[int]
    EVENT_FIELD_NUMBER: _ClassVar[int]
    cursor: bytes
    event: Event
    def __init__(self, cursor: _Optional[bytes] = ..., event: _Optional[_Union[Event, _Mapping]] = ...) -> None: ...

class Event(_message.Message):
    __slots__ = ("actor", "at", "repo_created", "ref_moved", "review_submitted", "voted", "commented", "thread_resolved", "stack_set", "workspace_changed", "workspace_deleted", "intent_changed", "relation_changed", "policy_changed", "version_stored", "status_changed", "landed", "land_requested", "land_status_changed", "quarantine_changed", "incident_changed", "trunk_health_changed", "reverts_requested")
    ACTOR_FIELD_NUMBER: _ClassVar[int]
    AT_FIELD_NUMBER: _ClassVar[int]
    REPO_CREATED_FIELD_NUMBER: _ClassVar[int]
    REF_MOVED_FIELD_NUMBER: _ClassVar[int]
    REVIEW_SUBMITTED_FIELD_NUMBER: _ClassVar[int]
    VOTED_FIELD_NUMBER: _ClassVar[int]
    COMMENTED_FIELD_NUMBER: _ClassVar[int]
    THREAD_RESOLVED_FIELD_NUMBER: _ClassVar[int]
    STACK_SET_FIELD_NUMBER: _ClassVar[int]
    WORKSPACE_CHANGED_FIELD_NUMBER: _ClassVar[int]
    WORKSPACE_DELETED_FIELD_NUMBER: _ClassVar[int]
    INTENT_CHANGED_FIELD_NUMBER: _ClassVar[int]
    RELATION_CHANGED_FIELD_NUMBER: _ClassVar[int]
    POLICY_CHANGED_FIELD_NUMBER: _ClassVar[int]
    VERSION_STORED_FIELD_NUMBER: _ClassVar[int]
    STATUS_CHANGED_FIELD_NUMBER: _ClassVar[int]
    LANDED_FIELD_NUMBER: _ClassVar[int]
    LAND_REQUESTED_FIELD_NUMBER: _ClassVar[int]
    LAND_STATUS_CHANGED_FIELD_NUMBER: _ClassVar[int]
    QUARANTINE_CHANGED_FIELD_NUMBER: _ClassVar[int]
    INCIDENT_CHANGED_FIELD_NUMBER: _ClassVar[int]
    TRUNK_HEALTH_CHANGED_FIELD_NUMBER: _ClassVar[int]
    REVERTS_REQUESTED_FIELD_NUMBER: _ClassVar[int]
    actor: _types_pb2.Principal
    at: int
    repo_created: RepoCreatedEvent
    ref_moved: RefMovedEvent
    review_submitted: ReviewSubmittedEvent
    voted: VotedEvent
    commented: CommentedEvent
    thread_resolved: ThreadResolvedEvent
    stack_set: StackSetEvent
    workspace_changed: WorkspaceChangedEvent
    workspace_deleted: WorkspaceDeletedEvent
    intent_changed: IntentChangedEvent
    relation_changed: RelationChangedEvent
    policy_changed: PolicyChangedEvent
    version_stored: VersionStoredEvent
    status_changed: StatusChangedEvent
    landed: LandedEvent
    land_requested: LandRequestedEvent
    land_status_changed: LandStatusChangedEvent
    quarantine_changed: QuarantineChangedEvent
    incident_changed: IncidentChangedEvent
    trunk_health_changed: TrunkHealthChangedEvent
    reverts_requested: RevertsRequestedEvent
    def __init__(self, actor: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., at: _Optional[int] = ..., repo_created: _Optional[_Union[RepoCreatedEvent, _Mapping]] = ..., ref_moved: _Optional[_Union[RefMovedEvent, _Mapping]] = ..., review_submitted: _Optional[_Union[ReviewSubmittedEvent, _Mapping]] = ..., voted: _Optional[_Union[VotedEvent, _Mapping]] = ..., commented: _Optional[_Union[CommentedEvent, _Mapping]] = ..., thread_resolved: _Optional[_Union[ThreadResolvedEvent, _Mapping]] = ..., stack_set: _Optional[_Union[StackSetEvent, _Mapping]] = ..., workspace_changed: _Optional[_Union[WorkspaceChangedEvent, _Mapping]] = ..., workspace_deleted: _Optional[_Union[WorkspaceDeletedEvent, _Mapping]] = ..., intent_changed: _Optional[_Union[IntentChangedEvent, _Mapping]] = ..., relation_changed: _Optional[_Union[RelationChangedEvent, _Mapping]] = ..., policy_changed: _Optional[_Union[PolicyChangedEvent, _Mapping]] = ..., version_stored: _Optional[_Union[VersionStoredEvent, _Mapping]] = ..., status_changed: _Optional[_Union[StatusChangedEvent, _Mapping]] = ..., landed: _Optional[_Union[LandedEvent, _Mapping]] = ..., land_requested: _Optional[_Union[LandRequestedEvent, _Mapping]] = ..., land_status_changed: _Optional[_Union[LandStatusChangedEvent, _Mapping]] = ..., quarantine_changed: _Optional[_Union[QuarantineChangedEvent, _Mapping]] = ..., incident_changed: _Optional[_Union[IncidentChangedEvent, _Mapping]] = ..., trunk_health_changed: _Optional[_Union[TrunkHealthChangedEvent, _Mapping]] = ..., reverts_requested: _Optional[_Union[RevertsRequestedEvent, _Mapping]] = ...) -> None: ...

class RepoCreatedEvent(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class RefMovedEvent(_message.Message):
    __slots__ = ("name", "old", "new")
    NAME_FIELD_NUMBER: _ClassVar[int]
    OLD_FIELD_NUMBER: _ClassVar[int]
    NEW_FIELD_NUMBER: _ClassVar[int]
    name: str
    old: bytes
    new: bytes
    def __init__(self, name: _Optional[str] = ..., old: _Optional[bytes] = ..., new: _Optional[bytes] = ...) -> None: ...

class ReviewSubmittedEvent(_message.Message):
    __slots__ = ("change_id", "version")
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    change_id: bytes
    version: bytes
    def __init__(self, change_id: _Optional[bytes] = ..., version: _Optional[bytes] = ...) -> None: ...

class VotedEvent(_message.Message):
    __slots__ = ("change_id", "label", "value", "version")
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    LABEL_FIELD_NUMBER: _ClassVar[int]
    VALUE_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    change_id: bytes
    label: str
    value: int
    version: bytes
    def __init__(self, change_id: _Optional[bytes] = ..., label: _Optional[str] = ..., value: _Optional[int] = ..., version: _Optional[bytes] = ...) -> None: ...

class CommentedEvent(_message.Message):
    __slots__ = ("change_id", "thread_id", "new_thread")
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    THREAD_ID_FIELD_NUMBER: _ClassVar[int]
    NEW_THREAD_FIELD_NUMBER: _ClassVar[int]
    change_id: bytes
    thread_id: str
    new_thread: bool
    def __init__(self, change_id: _Optional[bytes] = ..., thread_id: _Optional[str] = ..., new_thread: _Optional[bool] = ...) -> None: ...

class ThreadResolvedEvent(_message.Message):
    __slots__ = ("change_id", "thread_id", "resolved")
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    THREAD_ID_FIELD_NUMBER: _ClassVar[int]
    RESOLVED_FIELD_NUMBER: _ClassVar[int]
    change_id: bytes
    thread_id: str
    resolved: bool
    def __init__(self, change_id: _Optional[bytes] = ..., thread_id: _Optional[str] = ..., resolved: _Optional[bool] = ...) -> None: ...

class StackSetEvent(_message.Message):
    __slots__ = ("change_id", "stack")
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    STACK_FIELD_NUMBER: _ClassVar[int]
    change_id: bytes
    stack: _containers.RepeatedScalarFieldContainer[bytes]
    def __init__(self, change_id: _Optional[bytes] = ..., stack: _Optional[_Iterable[bytes]] = ...) -> None: ...

class WorkspaceChangedEvent(_message.Message):
    __slots__ = ("workspace_id", "operation_id")
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    OPERATION_ID_FIELD_NUMBER: _ClassVar[int]
    workspace_id: bytes
    operation_id: bytes
    def __init__(self, workspace_id: _Optional[bytes] = ..., operation_id: _Optional[bytes] = ...) -> None: ...

class WorkspaceDeletedEvent(_message.Message):
    __slots__ = ("workspace_id",)
    WORKSPACE_ID_FIELD_NUMBER: _ClassVar[int]
    workspace_id: bytes
    def __init__(self, workspace_id: _Optional[bytes] = ...) -> None: ...

class IntentChangedEvent(_message.Message):
    __slots__ = ("intent_id",)
    INTENT_ID_FIELD_NUMBER: _ClassVar[int]
    intent_id: bytes
    def __init__(self, intent_id: _Optional[bytes] = ...) -> None: ...

class RelationChangedEvent(_message.Message):
    __slots__ = ("object", "relation", "subject", "added")
    OBJECT_FIELD_NUMBER: _ClassVar[int]
    RELATION_FIELD_NUMBER: _ClassVar[int]
    SUBJECT_FIELD_NUMBER: _ClassVar[int]
    ADDED_FIELD_NUMBER: _ClassVar[int]
    object: str
    relation: str
    subject: str
    added: bool
    def __init__(self, object: _Optional[str] = ..., relation: _Optional[str] = ..., subject: _Optional[str] = ..., added: _Optional[bool] = ...) -> None: ...

class PolicyChangedEvent(_message.Message):
    __slots__ = ("name",)
    NAME_FIELD_NUMBER: _ClassVar[int]
    name: str
    def __init__(self, name: _Optional[str] = ...) -> None: ...

class QuarantineChangedEvent(_message.Message):
    __slots__ = ("target", "quarantined")
    TARGET_FIELD_NUMBER: _ClassVar[int]
    QUARANTINED_FIELD_NUMBER: _ClassVar[int]
    target: str
    quarantined: bool
    def __init__(self, target: _Optional[str] = ..., quarantined: _Optional[bool] = ...) -> None: ...

class IncidentChangedEvent(_message.Message):
    __slots__ = ("incident", "culprit", "state")
    INCIDENT_FIELD_NUMBER: _ClassVar[int]
    CULPRIT_FIELD_NUMBER: _ClassVar[int]
    STATE_FIELD_NUMBER: _ClassVar[int]
    incident: str
    culprit: int
    state: str
    def __init__(self, incident: _Optional[str] = ..., culprit: _Optional[int] = ..., state: _Optional[str] = ...) -> None: ...

class TrunkHealthChangedEvent(_message.Message):
    __slots__ = ("state", "frozen")
    STATE_FIELD_NUMBER: _ClassVar[int]
    FROZEN_FIELD_NUMBER: _ClassVar[int]
    state: str
    frozen: bool
    def __init__(self, state: _Optional[str] = ..., frozen: _Optional[bool] = ...) -> None: ...

class RevertsRequestedEvent(_message.Message):
    __slots__ = ("selector", "count")
    SELECTOR_FIELD_NUMBER: _ClassVar[int]
    COUNT_FIELD_NUMBER: _ClassVar[int]
    selector: str
    count: int
    def __init__(self, selector: _Optional[str] = ..., count: _Optional[int] = ...) -> None: ...

class VersionStoredEvent(_message.Message):
    __slots__ = ("version", "change_id")
    VERSION_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    version: bytes
    change_id: bytes
    def __init__(self, version: _Optional[bytes] = ..., change_id: _Optional[bytes] = ...) -> None: ...

class StatusChangedEvent(_message.Message):
    __slots__ = ("change_id", "to")
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    FROM_FIELD_NUMBER: _ClassVar[int]
    TO_FIELD_NUMBER: _ClassVar[int]
    change_id: bytes
    to: _types_pb2.ReviewStatus
    def __init__(self, change_id: _Optional[bytes] = ..., to: _Optional[_Union[_types_pb2.ReviewStatus, str]] = ..., **kwargs) -> None: ...

class LandedEvent(_message.Message):
    __slots__ = ("seq", "version", "change_id")
    SEQ_FIELD_NUMBER: _ClassVar[int]
    VERSION_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    seq: int
    version: bytes
    change_id: bytes
    def __init__(self, seq: _Optional[int] = ..., version: _Optional[bytes] = ..., change_id: _Optional[bytes] = ...) -> None: ...

class LandRequestedEvent(_message.Message):
    __slots__ = ("ticket", "change_id", "lane")
    TICKET_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    LANE_FIELD_NUMBER: _ClassVar[int]
    ticket: str
    change_id: bytes
    lane: str
    def __init__(self, ticket: _Optional[str] = ..., change_id: _Optional[bytes] = ..., lane: _Optional[str] = ...) -> None: ...

class LandStatusChangedEvent(_message.Message):
    __slots__ = ("ticket", "change_id", "status", "reason", "seq", "conflict_paths")
    TICKET_FIELD_NUMBER: _ClassVar[int]
    CHANGE_ID_FIELD_NUMBER: _ClassVar[int]
    STATUS_FIELD_NUMBER: _ClassVar[int]
    REASON_FIELD_NUMBER: _ClassVar[int]
    SEQ_FIELD_NUMBER: _ClassVar[int]
    CONFLICT_PATHS_FIELD_NUMBER: _ClassVar[int]
    ticket: str
    change_id: bytes
    status: str
    reason: str
    seq: int
    conflict_paths: _containers.RepeatedScalarFieldContainer[str]
    def __init__(self, ticket: _Optional[str] = ..., change_id: _Optional[bytes] = ..., status: _Optional[str] = ..., reason: _Optional[str] = ..., seq: _Optional[int] = ..., conflict_paths: _Optional[_Iterable[str]] = ...) -> None: ...
