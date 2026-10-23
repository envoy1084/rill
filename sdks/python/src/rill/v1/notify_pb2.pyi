from rill.v1 import types_pb2 as _types_pb2
from google.protobuf.internal import containers as _containers
from google.protobuf import descriptor as _descriptor
from google.protobuf import message as _message
from collections.abc import Iterable as _Iterable, Mapping as _Mapping
from typing import ClassVar as _ClassVar, Optional as _Optional, Union as _Union

DESCRIPTOR: _descriptor.FileDescriptor

class Webhook(_message.Message):
    __slots__ = ("name", "url", "secret", "events", "created_by", "created_at")
    NAME_FIELD_NUMBER: _ClassVar[int]
    URL_FIELD_NUMBER: _ClassVar[int]
    SECRET_FIELD_NUMBER: _ClassVar[int]
    EVENTS_FIELD_NUMBER: _ClassVar[int]
    CREATED_BY_FIELD_NUMBER: _ClassVar[int]
    CREATED_AT_FIELD_NUMBER: _ClassVar[int]
    name: str
    url: str
    secret: str
    events: _containers.RepeatedScalarFieldContainer[str]
    created_by: _types_pb2.Principal
    created_at: int
    def __init__(self, name: _Optional[str] = ..., url: _Optional[str] = ..., secret: _Optional[str] = ..., events: _Optional[_Iterable[str]] = ..., created_by: _Optional[_Union[_types_pb2.Principal, _Mapping]] = ..., created_at: _Optional[int] = ...) -> None: ...

class ListWebhooksRequest(_message.Message):
    __slots__ = ("parent",)
    PARENT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    def __init__(self, parent: _Optional[str] = ...) -> None: ...

class ListWebhooksResponse(_message.Message):
    __slots__ = ("webhooks",)
    WEBHOOKS_FIELD_NUMBER: _ClassVar[int]
    webhooks: _containers.RepeatedCompositeFieldContainer[Webhook]
    def __init__(self, webhooks: _Optional[_Iterable[_Union[Webhook, _Mapping]]] = ...) -> None: ...

class CreateWebhookRequest(_message.Message):
    __slots__ = ("request_id", "parent", "webhook")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    WEBHOOK_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    webhook: Webhook
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., webhook: _Optional[_Union[Webhook, _Mapping]] = ...) -> None: ...

class CreateWebhookResponse(_message.Message):
    __slots__ = ("webhook",)
    WEBHOOK_FIELD_NUMBER: _ClassVar[int]
    webhook: Webhook
    def __init__(self, webhook: _Optional[_Union[Webhook, _Mapping]] = ...) -> None: ...

class DeleteWebhookRequest(_message.Message):
    __slots__ = ("request_id", "name")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    NAME_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    name: str
    def __init__(self, request_id: _Optional[str] = ..., name: _Optional[str] = ...) -> None: ...

class DeleteWebhookResponse(_message.Message):
    __slots__ = ()
    def __init__(self) -> None: ...

class NotificationChannel(_message.Message):
    __slots__ = ("slack", "teams", "email")
    SLACK_FIELD_NUMBER: _ClassVar[int]
    TEAMS_FIELD_NUMBER: _ClassVar[int]
    EMAIL_FIELD_NUMBER: _ClassVar[int]
    slack: str
    teams: str
    email: str
    def __init__(self, slack: _Optional[str] = ..., teams: _Optional[str] = ..., email: _Optional[str] = ...) -> None: ...

class NotificationPrefs(_message.Message):
    __slots__ = ("channels", "topics", "digest")
    CHANNELS_FIELD_NUMBER: _ClassVar[int]
    TOPICS_FIELD_NUMBER: _ClassVar[int]
    DIGEST_FIELD_NUMBER: _ClassVar[int]
    channels: _containers.RepeatedCompositeFieldContainer[NotificationChannel]
    topics: _containers.RepeatedScalarFieldContainer[str]
    digest: bool
    def __init__(self, channels: _Optional[_Iterable[_Union[NotificationChannel, _Mapping]]] = ..., topics: _Optional[_Iterable[str]] = ..., digest: _Optional[bool] = ...) -> None: ...

class GetNotificationPrefsRequest(_message.Message):
    __slots__ = ("parent",)
    PARENT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    def __init__(self, parent: _Optional[str] = ...) -> None: ...

class GetNotificationPrefsResponse(_message.Message):
    __slots__ = ("prefs",)
    PREFS_FIELD_NUMBER: _ClassVar[int]
    prefs: NotificationPrefs
    def __init__(self, prefs: _Optional[_Union[NotificationPrefs, _Mapping]] = ...) -> None: ...

class UpdateNotificationPrefsRequest(_message.Message):
    __slots__ = ("request_id", "parent", "prefs")
    REQUEST_ID_FIELD_NUMBER: _ClassVar[int]
    PARENT_FIELD_NUMBER: _ClassVar[int]
    PREFS_FIELD_NUMBER: _ClassVar[int]
    request_id: str
    parent: str
    prefs: NotificationPrefs
    def __init__(self, request_id: _Optional[str] = ..., parent: _Optional[str] = ..., prefs: _Optional[_Union[NotificationPrefs, _Mapping]] = ...) -> None: ...

class UpdateNotificationPrefsResponse(_message.Message):
    __slots__ = ("prefs",)
    PREFS_FIELD_NUMBER: _ClassVar[int]
    prefs: NotificationPrefs
    def __init__(self, prefs: _Optional[_Union[NotificationPrefs, _Mapping]] = ...) -> None: ...

class SendTestNotificationRequest(_message.Message):
    __slots__ = ("parent",)
    PARENT_FIELD_NUMBER: _ClassVar[int]
    parent: str
    def __init__(self, parent: _Optional[str] = ...) -> None: ...

class SendTestNotificationResponse(_message.Message):
    __slots__ = ("queued",)
    QUEUED_FIELD_NUMBER: _ClassVar[int]
    queued: int
    def __init__(self, queued: _Optional[int] = ...) -> None: ...
