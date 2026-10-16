"""An ergonomic client that carries default routing parameters.

``Client`` is a thin convenience layer over :func:`ilink_sdk.call`. Every method
accepts an optional override of the routing parameters (``account_id``,
``base_url``, ``cdn_base_url``, ``route_tag``, ``bot_agent``, ``timeout_ms``),
and the ones set on the client are applied to every call.
"""

from __future__ import annotations

from typing import Any, Mapping, Optional

from ._call import call as _call

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

    def call(self, op: str, params: Optional[Mapping[str, Any]] = None) -> Any:
        """Run any operation with the client's routing parameters merged in."""
        merged: dict[str, Any] = dict(self._defaults)
        if params:
            merged.update(params)
        return _call(op, merged)

    # -- meta ---------------------------------------------------------------
    def version(self) -> str:
        return self.call("version")

    def paths(self) -> Mapping[str, Any]:
        return self.call("paths")

    # -- accounts -----------------------------------------------------------
    def accounts(self) -> list[Mapping[str, Any]]:
        return self.call("accounts.list")

    def resolve_account(self, account_id: str) -> Mapping[str, Any]:
        return self.call("account.resolve", {"account_id": account_id})

    def save_account(
        self,
        account_id: str,
        token: Optional[str] = None,
        base_url: Optional[str] = None,
        user_id: Optional[str] = None,
    ) -> Mapping[str, Any]:
        return self.call(
            "account.save",
            {"account_id": account_id, "token": token, "base_url": base_url, "user_id": user_id},
        )

    def clear_account(self, account_id: str) -> Mapping[str, Any]:
        return self.call("account.clear", {"account_id": account_id})

    # -- QR login -----------------------------------------------------------
    def login_start(
        self, base_url: Optional[str] = None, bot_type: Optional[str] = None
    ) -> Mapping[str, Any]:
        return self.call("login.start", {"base_url": base_url, "bot_type": bot_type})

    def login_poll(self, session_key: str) -> Mapping[str, Any]:
        return self.call("login.poll", {"session_key": session_key})

    def login_complete(
        self,
        session_key: str,
        timeout_secs: int = 480,
        verify_code: Optional[str] = None,
    ) -> Mapping[str, Any]:
        """Wait for confirmation and persist the credentials."""
        return self.call(
            "login.complete",
            {"session_key": session_key, "timeout_secs": timeout_secs, "verify_code": verify_code},
        )

    # -- outbound -----------------------------------------------------------
    def send_text(
        self,
        to: str,
        text: str,
        context_token: Optional[str] = None,
        run_id: Optional[str] = None,
    ) -> Mapping[str, Any]:
        return self.call(
            "send.text", {"to": to, "text": text, "context_token": context_token, "run_id": run_id}
        )

    def send_file(
        self, to: str, path: str, caption: Optional[str] = None, context_token: Optional[str] = None
    ) -> Mapping[str, Any]:
        return self.call(
            "send.file",
            {"to": to, "path": path, "caption": caption, "context_token": context_token},
        )

    def send_bytes(
        self,
        to: str,
        file_name: str,
        data_base64: str,
        caption: Optional[str] = None,
    ) -> Mapping[str, Any]:
        return self.call(
            "send.bytes",
            {"to": to, "file_name": file_name, "data_base64": data_base64, "caption": caption},
        )

    def send_item(self, to: str, item: Mapping[str, Any], context_token: Optional[str] = None) -> Mapping[str, Any]:
        return self.call("send.item", {"to": to, "item": item, "context_token": context_token})

    # -- inbound / API ------------------------------------------------------
    def get_updates(
        self, get_updates_buf: Optional[str] = None, timeout_ms: Optional[int] = None
    ) -> Mapping[str, Any]:
        return self.call(
            "get_updates", {"get_updates_buf": get_updates_buf, "timeout_ms": timeout_ms}
        )

    def get_config(self, user_id: str, context_token: Optional[str] = None) -> Mapping[str, Any]:
        return self.call("get_config", {"user_id": user_id, "context_token": context_token})

    def send_typing(self, user_id: str, typing_ticket: str, status: str = "typing") -> Mapping[str, Any]:
        return self.call(
            "send_typing", {"user_id": user_id, "typing_ticket": typing_ticket, "status": status}
        )

    def notify(self, action: str = "start") -> Mapping[str, Any]:
        return self.call("notify", {"action": action})

    # -- media --------------------------------------------------------------
    def upload(self, to: str, path: str, media_type: str = "file") -> Mapping[str, Any]:
        return self.call("media.upload", {"to": to, "path": path, "media_type": media_type})

    def download(
        self, media: Mapping[str, Any], aes_key: Optional[str] = None, dest_path: Optional[str] = None
    ) -> Mapping[str, Any]:
        return self.call(
            "media.download", {"media": media, "aes_key": aes_key, "dest_path": dest_path}
        )

    def fetch(self, url: str, dest_path: Optional[str] = None) -> Mapping[str, Any]:
        return self.call("media.fetch", {"url": url, "dest_path": dest_path})

    # -- helpers ------------------------------------------------------------
    def mime_info(self, path: str) -> Mapping[str, Any]:
        return self.call("mime.info", {"path": path})

    def session_status(self, account_id: Optional[str] = None) -> Mapping[str, Any]:
        return self.call("session.status", {"account_id": account_id} if account_id else None)
