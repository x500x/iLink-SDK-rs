"""An ergonomic client that carries default routing parameters.

``Client`` is a convenience layer over :func:`ilink_sdk.call`. Every method
returns a typed :mod:`ilink_sdk.models` object rather than a bare ``dict``, and
accepts an optional override of the routing parameters (``account_id``,
``base_url``, ``cdn_base_url``, ``route_tag``, ``bot_agent``, ``timeout_ms``);
the ones set on the client are applied to every call.

>>> client = Client(account_id="cfe9a2017e77-im-bot")
>>> result = client.send_text("someone@im.wechat", "hi")
>>> result.message_id
'...'
"""

from __future__ import annotations

import base64
from typing import Any, List, Mapping, Optional, Type, TypeVar, Union

from . import models
from ._call import call as _call
from .models import from_dict

T = TypeVar("T")

_ROUTING = ("account_id", "base_url", "cdn_base_url", "route_tag", "bot_agent", "timeout_ms")


class Client:
    def __init__(
        self,
        account_id: Optional[str] = None,
        base_url: Optional[str] = None,
        cdn_base_url: Optional[str] = None,
        route_tag: Optional[str] = None,
        bot_agent: Optional[str] = None,
        timeout_ms: Optional[int] = None,
    ) -> None:
        defaults: dict[str, Any] = {
            "account_id": account_id,
            "base_url": base_url,
            "cdn_base_url": cdn_base_url,
            "route_tag": route_tag,
            "bot_agent": bot_agent,
            "timeout_ms": timeout_ms,
        }
        self._defaults = {k: v for k, v in defaults.items() if v is not None}

    # -- plumbing -----------------------------------------------------------
    def call(self, op: str, params: Optional[Mapping[str, Any]] = None) -> Any:
        """Run any operation with the client's routing parameters merged in.

        Returns the decoded ``result`` as plain Python data. Use
        :meth:`call_into` when you want a typed model.
        """
        merged: dict[str, Any] = dict(self._defaults)
        if params:
            merged.update(params)
        return _call(op, merged)

    def call_into(
        self,
        op: str,
        model: Type[T],
        params: Optional[Mapping[str, Any]] = None,
    ) -> T:
        """Run any operation and decode the result into ``model``."""
        return from_dict(model, self.call(op, params))

    # -- meta ---------------------------------------------------------------
    def version(self) -> str:
        return self.call("version")

    def paths(self) -> models.Paths:
        return self.call_into("paths", models.Paths)

    # -- accounts -----------------------------------------------------------
    def accounts(self) -> List[models.Account]:
        return [from_dict(models.Account, item) for item in self.call("accounts.list")]

    def resolve_account(self, account_id: str) -> models.Account:
        return self.call_into("account.resolve", models.Account, {"account_id": account_id})

    def save_account(
        self,
        account_id: str,
        token: Optional[str] = None,
        base_url: Optional[str] = None,
        user_id: Optional[str] = None,
    ) -> models.Account:
        return self.call_into(
            "account.save",
            models.Account,
            {"account_id": account_id, "token": token, "base_url": base_url, "user_id": user_id},
        )

    def clear_account(self, account_id: str) -> None:
        """Forget an account and all of its local state."""
        self.call("account.clear", {"account_id": account_id})

    # -- QR login -----------------------------------------------------------
    def login_start(
        self, base_url: Optional[str] = None, bot_type: Optional[str] = None
    ) -> models.LoginStart:
        """Begin a QR login; display ``qrcode_url`` to the user."""
        return self.call_into(
            "login.start", models.LoginStart, {"base_url": base_url, "bot_type": bot_type}
        )

    def login_poll(self, session_key: str) -> models.LoginPoll:
        """Non-blocking status check for a login session."""
        return self.call_into("login.poll", models.LoginPoll, {"session_key": session_key})

    def login_submit_verify_code(self, session_key: str, code: str) -> None:
        self.call("login.submit_verify_code", {"session_key": session_key, "code": code})

    def login_refresh(self, session_key: str) -> models.LoginStart:
        return self.call_into("login.refresh", models.LoginStart, {"session_key": session_key})

    def login_wait(
        self,
        session_key: str,
        timeout_secs: int = 480,
        verify_code: Optional[str] = None,
    ) -> models.LoginResult:
        """Wait for confirmation without persisting the credentials."""
        return self.call_into(
            "login.wait",
            models.LoginResult,
            {
                "session_key": session_key,
                "timeout_secs": timeout_secs,
                "verify_code": verify_code,
            },
        )

    def login_complete(
        self,
        session_key: str,
        timeout_secs: int = 480,
        verify_code: Optional[str] = None,
    ) -> models.LoginResult:
        """Wait for confirmation and persist the credentials."""
        return self.call_into(
            "login.complete",
            models.LoginResult,
            {
                "session_key": session_key,
                "timeout_secs": timeout_secs,
                "verify_code": verify_code,
            },
        )

    # -- outbound -----------------------------------------------------------
    def send_text(
        self,
        to: str,
        text: str,
        context_token: Optional[str] = None,
        run_id: Optional[str] = None,
    ) -> models.SendResult:
        return self.call_into(
            "send.text",
            models.SendResult,
            {"to": to, "text": text, "context_token": context_token, "run_id": run_id},
        )

    def send_file(
        self,
        to: str,
        path: str,
        caption: Optional[str] = None,
        context_token: Optional[str] = None,
    ) -> models.SendResult:
        """Send a local image / video / file, chosen by its MIME type."""
        return self.call_into(
            "send.file",
            models.SendResult,
            {"to": to, "path": path, "caption": caption, "context_token": context_token},
        )

    def send_bytes(
        self,
        to: str,
        file_name: str,
        data: Union[bytes, bytearray, str],
        caption: Optional[str] = None,
        context_token: Optional[str] = None,
    ) -> models.SendResult:
        """Send in-memory bytes; ``data`` is base64 encoded for the wire."""
        if isinstance(data, str):
            encoded = data
        else:
            encoded = base64.b64encode(bytes(data)).decode("ascii")
        return self.call_into(
            "send.bytes",
            models.SendResult,
            {
                "to": to,
                "file_name": file_name,
                "data_base64": encoded,
                "caption": caption,
                "context_token": context_token,
            },
        )

    def send_item(
        self,
        to: str,
        item: Union[Mapping[str, Any], models.MessageItem],
        context_token: Optional[str] = None,
    ) -> models.SendResult:
        """Send a raw protocol item (advanced)."""
        raw = item if isinstance(item, Mapping) else _dataclass_to_dict(item)
        return self.call_into(
            "send.item",
            models.SendResult,
            {"to": to, "item": raw, "context_token": context_token},
        )

    # -- inbound / API ------------------------------------------------------
    def get_updates(
        self, get_updates_buf: Optional[str] = None, timeout_ms: Optional[int] = None
    ) -> models.Updates:
        """Long-poll for inbound messages; pass the previous cursor back in."""
        return self.call_into(
            "get_updates",
            models.Updates,
            {"get_updates_buf": get_updates_buf, "timeout_ms": timeout_ms},
        )

    def get_config(self, user_id: str, context_token: Optional[str] = None) -> models.GetConfig:
        return self.call_into(
            "get_config",
            models.GetConfig,
            {"user_id": user_id, "context_token": context_token},
        )

    def send_typing(
        self, user_id: str, typing_ticket: str, status: str = "typing"
    ) -> None:
        self.call(
            "send_typing",
            {"user_id": user_id, "typing_ticket": typing_ticket, "status": status},
        )

    def notify(self, action: str = "start") -> None:
        """Lifecycle ping (``start`` or ``stop``)."""
        self.call("notify", {"action": action})

    def context_to_message(
        self, message: Union[Mapping[str, Any], models.WeixinMessage], account_id: Optional[str] = None
    ) -> models.MsgContext:
        """Parse a raw inbound message into a :class:`MsgContext`."""
        raw = message if isinstance(message, Mapping) else _dataclass_to_dict(message)
        return self.call_into(
            "context.to_message", models.MsgContext, {"message": raw, "account_id": account_id}
        )

    def message_body(self, items: List[Union[Mapping[str, Any], models.MessageItem]]) -> str:
        """Extract the display body from a list of message items."""
        raw = [i if isinstance(i, Mapping) else _dataclass_to_dict(i) for i in items]
        return self.call("message.body", {"items": raw})["body"]

    # -- media --------------------------------------------------------------
    def upload(self, to: str, path: str, media_type: str = "file") -> models.UploadedFile:
        """Upload a file to the CDN and return its reference."""
        return self.call_into(
            "media.upload",
            models.UploadedFile,
            {"to": to, "path": path, "media_type": media_type},
        )

    def download(
        self,
        media: Union[Mapping[str, Any], models.CdnMedia],
        aes_key: Optional[str] = None,
        dest_path: Optional[str] = None,
    ) -> models.MediaFetch:
        """Download a CDN reference, decrypting it when ``aes_key`` is given."""
        raw = media if isinstance(media, Mapping) else _dataclass_to_dict(media)
        return self.call_into(
            "media.download",
            models.MediaFetch,
            {"media": raw, "aes_key": aes_key, "dest_path": dest_path},
        )

    def fetch(self, url: str, dest_path: Optional[str] = None) -> models.MediaFetch:
        return self.call_into(
            "media.fetch", models.MediaFetch, {"url": url, "dest_path": dest_path}
        )

    def download_item(
        self,
        item: Union[Mapping[str, Any], models.MessageItem],
        dest_dir: Optional[str] = None,
    ) -> models.InboundMedia:
        """Download every media referenced by an inbound message item."""
        raw = item if isinstance(item, Mapping) else _dataclass_to_dict(item)
        return self.call_into(
            "media.download_item", models.InboundMedia, {"item": raw, "dest_dir": dest_dir}
        )

    # -- quote cache --------------------------------------------------------
    def quote_policy(self) -> models.QuotePolicy:
        return self.call_into("quote.policy", models.QuotePolicy)

    def quote_find(
        self, account_id: str, conversation_id: str, message_id: str
    ) -> Optional[models.QuoteRecord]:
        result = self.call(
            "quote.find",
            {
                "account_id": account_id,
                "conversation_id": conversation_id,
                "message_id": message_id,
            },
        )
        return None if result is None else from_dict(models.QuoteRecord, result)

    def quote_put(
        self,
        account_id: str,
        conversation_id: str,
        message_id: str,
        body: str = "",
        direction: str = "inbound",
        source_media_path: Optional[str] = None,
        media_mime: Optional[str] = None,
        media_name: Optional[str] = None,
        created_at: int = 0,
    ) -> None:
        self.call(
            "quote.put",
            {
                "account_id": account_id,
                "conversation_id": conversation_id,
                "message_id": message_id,
                "body": body,
                "direction": direction,
                "source_media_path": source_media_path,
                "media_mime": media_mime,
                "media_name": media_name,
                "created_at": created_at,
            },
        )

    def quote_gc(self) -> None:
        self.call("quote.gc")

    def quote_media_dir(self, account_id: str) -> str:
        return self.call("quote.media_dir", {"account_id": account_id})["path"]

    # -- context tokens -----------------------------------------------------
    def context_get(self, account_id: str, user_id: str) -> Optional[str]:
        result = self.call("context.get", {"account_id": account_id, "user_id": user_id})
        return None if result is None else result["token"]

    def context_set(self, account_id: str, user_id: str, token: str) -> None:
        self.call("context.set", {"account_id": account_id, "user_id": user_id, "token": token})

    def context_clear(self, account_id: str) -> None:
        self.call("context.clear", {"account_id": account_id})

    def context_restore(self, account_id: str) -> int:
        return self.call("context.restore", {"account_id": account_id})["restored"]

    # -- sync cursor --------------------------------------------------------
    def sync_load(self, account_id: str) -> str:
        return self.call("sync.load", {"account_id": account_id})["get_updates_buf"]

    def sync_save(self, account_id: str, buf: str) -> None:
        self.call("sync.save", {"account_id": account_id, "buf": buf})

    def sync_clear(self, account_id: str) -> None:
        self.call("sync.clear", {"account_id": account_id})

    # -- session guard ------------------------------------------------------
    def session_status(self, account_id: Optional[str] = None) -> models.SessionStatus:
        return self.call_into(
            "session.status",
            models.SessionStatus,
            {"account_id": account_id} if account_id else None,
        )

    def session_pause(self, account_id: str) -> None:
        self.call("session.pause", {"account_id": account_id})

    def session_resume(self, account_id: str) -> None:
        self.call("session.resume", {"account_id": account_id})

    def session_assert(self, account_id: str) -> None:
        """Raise :class:`IlinkError` when the session is paused."""
        self.call("session.assert", {"account_id": account_id})

    # -- helpers ------------------------------------------------------------
    def mime_info(self, path: str) -> models.MimeInfo:
        return self.call_into("mime.info", models.MimeInfo, {"path": path})

    def mime_extension(self, mime: str) -> str:
        return self.call("mime.extension", {"mime": mime})["extension"]

    def generate_id(self, prefix: str = "ilink-sdk") -> str:
        return self.call("util.generate_id", {"prefix": prefix})["id"]

    def file_key(self) -> str:
        return self.call("util.file_key")["file_key"]

    def temp_name(self, prefix: str = "ilink-sdk", ext: str = "") -> str:
        return self.call("util.temp_name", {"prefix": prefix, "ext": ext})["name"]

    def config_load(self) -> models.ConfigLoad:
        return self.call_into("config.load", models.ConfigLoad)

    # -- crypto -------------------------------------------------------------
    def crypto_encrypt(self, key_hex: str, data: Union[bytes, bytearray, str]) -> models.CryptoResult:
        encoded = data if isinstance(data, str) else base64.b64encode(bytes(data)).decode("ascii")
        return self.call_into(
            "crypto.encrypt",
            models.CryptoResult,
            {"key_hex": key_hex, "data_base64": encoded},
        )

    def crypto_decrypt(self, key_hex: str, data_base64: str) -> bytes:
        result = self.call("crypto.decrypt", {"key_hex": key_hex, "data_base64": data_base64})
        return base64.b64decode(result["data_base64"])


def _dataclass_to_dict(obj: Any) -> Mapping[str, Any]:
    """Convert a model instance back into the wire dictionary."""
    import dataclasses

    out: dict[str, Any] = {}
    for f in dataclasses.fields(obj):
        value = getattr(obj, f.name)
        if value is None:
            continue
        key = f.metadata.get("json", f.name)
        if dataclasses.is_dataclass(value):
            value = _dataclass_to_dict(value)
        elif isinstance(value, list):
            value = [_dataclass_to_dict(v) if dataclasses.is_dataclass(v) else v for v in value]
        out[key] = value
    return out
