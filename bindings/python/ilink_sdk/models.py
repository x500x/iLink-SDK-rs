"""Typed result models for the iLink SDK Python bindings.

Every operation result is decoded into a :mod:`dataclasses` instance so callers
get attribute access and type hints instead of poking at dictionaries. The
models are tolerant: unknown JSON keys are ignored and missing keys fall back
to the declared defaults, so a newer server never breaks an older binding.

>>> from ilink_sdk import Client
>>> client = Client(account_id="cfe9a2017e77-im-bot")
>>> client.send_text("someone@im.wechat", "hi").message_id
'...'
"""

from __future__ import annotations

import dataclasses
import types
from dataclasses import dataclass, field
from typing import Any, List, Mapping, Optional, Union, get_args, get_origin, get_type_hints

__all__ = [
    "Account",
    "CdnMedia",
    "ConfigLoad",
    "CryptoResult",
    "FileItem",
    "GetConfig",
    "ImageItem",
    "InboundMedia",
    "LoginPoll",
    "LoginResult",
    "LoginStart",
    "MediaEntry",
    "MediaFetch",
    "MessageItem",
    "MimeInfo",
    "MsgContext",
    "PartialText",
    "Paths",
    "QrSession",
    "QuotePolicy",
    "QuoteRecord",
    "RefMessage",
    "SendResult",
    "SessionStatus",
    "TextItem",
    "Updates",
    "UploadedFile",
    "VideoItem",
    "VoiceItem",
    "WeixinMessage",
    "from_dict",
]

_UNION_TYPES = (Union, types.UnionType)


def _coerce(value: Any, hint: Any) -> Any:
    """Convert a decoded JSON value into the type described by ``hint``."""
    if value is None:
        return None
    origin = get_origin(hint)
    if origin is not None:
        args = get_args(hint)
        if origin in _UNION_TYPES:
            non_none = [a for a in args if a is not type(None)]
            return _coerce(value, non_none[0]) if non_none else value
        if origin in (list, List):
            item_hint = args[0] if args else Any
            return [_coerce(item, item_hint) for item in value]
        return value
    if dataclasses.is_dataclass(hint):
        return from_dict(hint, value)
    return value


def from_dict(cls: type, data: Optional[Mapping[str, Any]]) -> Any:
    """Build ``cls`` from a decoded JSON object, ignoring unknown keys.

    A field may declare the JSON key it maps to through dataclass metadata, so
    Python keywords such as ``from`` can be exposed as ``from_``::

        from_: Optional[str] = field(default=None, metadata={"json": "from"})
    """
    if data is None:
        return None
    hints = get_type_hints(cls)
    kwargs: dict[str, Any] = {}
    for f in dataclasses.fields(cls):
        key = f.metadata.get("json", f.name)
        if key in data:
            kwargs[f.name] = _coerce(data[key], hints.get(f.name, Any))
    return cls(**kwargs)


# ---------------------------------------------------------------------------
# Meta
# ---------------------------------------------------------------------------


@dataclass
class Paths:
    state_dir: str = ""
    sdk_state_dir: str = ""
    accounts_dir: str = ""
    config_path: str = ""


# ---------------------------------------------------------------------------
# Accounts
# ---------------------------------------------------------------------------


@dataclass
class Account:
    account_id: str = ""
    base_url: str = ""
    cdn_base_url: str = ""
    configured: bool = False
    enabled: bool = False
    user_id: Optional[str] = None
    name: Optional[str] = None
    route_tag: Optional[str] = None
    token_present: bool = False


@dataclass
class QrSession:
    token: str = ""
    bot_id: str = ""
    base_url: str = ""
    user_id: str = ""


@dataclass
class LoginStart:
    qrcode: str = ""
    qrcode_url: str = ""
    session_key: str = ""


@dataclass
class LoginPoll:
    kind: str = ""
    retry: Optional[int] = None
    session: Optional[QrSession] = None


@dataclass
class LoginResult:
    connected: bool = False
    already_connected: bool = False
    message: str = ""
    account_id: Optional[str] = None
    session: Optional[QrSession] = None


# ---------------------------------------------------------------------------
# Outbound
# ---------------------------------------------------------------------------


@dataclass
class SendResult:
    message_id: str = ""
    server_message_id: str = ""


# ---------------------------------------------------------------------------
# Wire types
# ---------------------------------------------------------------------------


@dataclass
class TextItem:
    text: Optional[str] = None


@dataclass
class CdnMedia:
    encrypt_query_param: Optional[str] = None
    aes_key: Optional[str] = None
    encrypt_type: Optional[int] = None
    full_url: Optional[str] = None


@dataclass
class ImageItem:
    media: Optional[CdnMedia] = None
    thumb_media: Optional[CdnMedia] = None
    aeskey: Optional[str] = None
    url: Optional[str] = None
    mid_size: Optional[int] = None
    thumb_size: Optional[int] = None
    thumb_height: Optional[int] = None
    thumb_width: Optional[int] = None
    hd_size: Optional[int] = None


@dataclass
class VoiceItem:
    media: Optional[CdnMedia] = None
    encode_type: Optional[int] = None
    bits_per_sample: Optional[int] = None
    sample_rate: Optional[int] = None
    playtime: Optional[int] = None
    text: Optional[str] = None


@dataclass
class FileItem:
    media: Optional[CdnMedia] = None
    file_name: Optional[str] = None
    md5: Optional[str] = None
    len: Optional[str] = None


@dataclass
class VideoItem:
    media: Optional[CdnMedia] = None
    video_size: Optional[int] = None
    play_length: Optional[int] = None
    video_md5: Optional[str] = None
    thumb_media: Optional[CdnMedia] = None
    thumb_size: Optional[int] = None
    thumb_height: Optional[int] = None
    thumb_width: Optional[int] = None


@dataclass
class PartialText:
    start: str = ""
    end: str = ""
    startindex: int = 0
    endindex: int = 0
    quotemd5: str = ""


@dataclass
class RefMessage:
    message_item: Optional["MessageItem"] = None
    title: Optional[str] = None
    svr_id: Optional[str] = None
    partial_text: Optional[PartialText] = None


@dataclass
class MessageItem:
    type: Optional[int] = None
    msg_id: Optional[str] = None
    create_time_ms: Optional[int] = None
    update_time_ms: Optional[int] = None
    is_completed: Optional[bool] = None
    ref_msg: Optional[RefMessage] = None
    text_item: Optional[TextItem] = None
    image_item: Optional[ImageItem] = None
    voice_item: Optional[VoiceItem] = None
    file_item: Optional[FileItem] = None
    video_item: Optional[VideoItem] = None

    @property
    def text(self) -> Optional[str]:
        """The text of a text item, when this is one."""
        return self.text_item.text if self.text_item else None


@dataclass
class WeixinMessage:
    seq: Optional[int] = None
    message_id: Optional[str] = None
    from_user_id: Optional[str] = None
    to_user_id: Optional[str] = None
    client_id: Optional[str] = None
    create_time_ms: Optional[int] = None
    update_time_ms: Optional[int] = None
    delete_time_ms: Optional[int] = None
    session_id: Optional[str] = None
    group_id: Optional[str] = None
    message_type: Optional[int] = None
    message_state: Optional[int] = None
    item_list: List[MessageItem] = field(default_factory=list)
    context_token: Optional[str] = None
    run_id: Optional[str] = None


@dataclass
class Updates:
    """Result of :meth:`Client.get_updates`."""

    ret: Optional[int] = None
    errcode: Optional[int] = None
    errmsg: Optional[str] = None
    msgs: List[WeixinMessage] = field(default_factory=list)
    get_updates_buf: Optional[str] = None
    longpolling_timeout_ms: Optional[int] = None

    @property
    def messages(self) -> List[WeixinMessage]:
        """Alias for :attr:`msgs`."""
        return self.msgs

    @property
    def is_error(self) -> bool:
        """``True`` when the server reported a business-level failure."""
        return bool(self.ret) or bool(self.errcode)


# ---------------------------------------------------------------------------
# Inbound context
# ---------------------------------------------------------------------------


@dataclass
class MediaEntry:
    path: str = ""
    content_type: Optional[str] = None
    file_name: Optional[str] = None
    message_id: Optional[str] = None


@dataclass
class MsgContext:
    body: str = ""
    from_: Optional[str] = field(default=None, metadata={"json": "from"})
    to: str = ""
    account_id: str = ""
    originating_channel: str = ""
    originating_to: str = ""
    message_sid: str = ""
    message_sid_full: Optional[str] = None
    timestamp: Optional[int] = None
    provider: str = ""
    chat_type: str = ""
    session_key: Optional[str] = None
    context_token: Optional[str] = None
    media_path: Optional[str] = None
    media_type: Optional[str] = None
    media_paths: List[str] = field(default_factory=list)
    media_types: List[str] = field(default_factory=list)
    media: List[MediaEntry] = field(default_factory=list)
    channel_prompt_context: List[str] = field(default_factory=list)
    reply_to_id: Optional[str] = None
    reply_to_body: Optional[str] = None
    reply_to_quote_text: Optional[str] = None
    reply_to_is_quote: Optional[bool] = None


# ---------------------------------------------------------------------------
# Media
# ---------------------------------------------------------------------------


@dataclass
class UploadedFile:
    filekey: str = ""
    download_param: str = ""
    aeskey_hex: str = ""
    aes_key_base64: str = ""
    file_size: int = 0
    file_size_ciphertext: int = 0


@dataclass
class InboundMedia:
    image_path: Optional[str] = None
    voice_path: Optional[str] = None
    voice_mime: Optional[str] = None
    file_path: Optional[str] = None
    file_mime: Optional[str] = None
    video_path: Optional[str] = None
    referenced: bool = False
    is_empty: bool = False

    @property
    def paths(self) -> List[str]:
        """Every downloaded local path, in a stable order."""
        return [p for p in (self.image_path, self.voice_path, self.file_path, self.video_path) if p]


@dataclass
class MediaFetch:
    bytes: int = 0
    path: Optional[str] = None
    data_base64: Optional[str] = None

    def data(self) -> Optional[bytes]:
        """The fetched payload when it was returned inline (base64 decoded)."""
        if self.data_base64 is None:
            return None
        import base64

        return base64.b64decode(self.data_base64)


# ---------------------------------------------------------------------------
# Quote cache
# ---------------------------------------------------------------------------


@dataclass
class QuotePolicy:
    enabled: bool = False
    retention_ms: int = 0
    max_messages_per_account: int = 0
    media_retention_ms: int = 0
    max_media_bytes_per_account: int = 0
    max_single_media_bytes: int = 0


@dataclass
class QuoteRecord:
    account_id: str = ""
    conversation_id: str = ""
    message_id: str = ""
    direction: str = "inbound"
    body: str = ""
    media_path: Optional[str] = None
    media_mime: Optional[str] = None
    media_name: Optional[str] = None
    media_size: Optional[int] = None
    created_at: int = 0


# ---------------------------------------------------------------------------
# Session / helpers
# ---------------------------------------------------------------------------


@dataclass
class SessionStatus:
    paused: bool = False
    remaining_ms: int = 0


@dataclass
class GetConfig:
    ret: Optional[int] = None
    errmsg: Optional[str] = None
    typing_ticket: Optional[str] = None

    @property
    def is_ok(self) -> bool:
        return self.ret == 0


@dataclass
class MimeInfo:
    mime: str = ""
    kind: str = ""
    extension: str = ""
    is_image: bool = False
    is_video: bool = False
    is_audio: bool = False


@dataclass
class ConfigLoad:
    config: Mapping[str, Any] = field(default_factory=dict)
    quote_cache_policy: Optional[QuotePolicy] = None


@dataclass
class CryptoResult:
    data_base64: str = ""
    padded_size: Optional[int] = None

    def data(self) -> bytes:
        """The ciphertext, base64 decoded."""
        import base64

        return base64.b64decode(self.data_base64)
