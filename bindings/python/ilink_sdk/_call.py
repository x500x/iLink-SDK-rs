"""Low-level, language-neutral access to the iLink SDK C ABI.

Everything the SDK can do is reachable through :func:`call`: pass an operation
name and a JSON-serialisable parameter mapping, get the operation result back as
a decoded Python object. Failures raise :class:`IlinkError` carrying the
``kind`` reported by the Rust error type.
"""

from __future__ import annotations

import ctypes
import json
from typing import Any, Mapping, Optional

from ._ffi import library


class IlinkError(Exception):
    """An operation failed. ``kind`` mirrors ``ilink_sdk::ErrorKind``."""

    def __init__(self, message: str, kind: str = "unknown") -> None:
        super().__init__(message)
        self.message = message
        self.kind = kind


def _take_string(ptr: int) -> str:
    """Copy a ``char*`` returned by the library and release it."""
    lib = library()
    try:
        return ctypes.string_at(ptr).decode("utf-8")
    finally:
        lib.ilink_string_free(ptr)


def version() -> str:
    """The SDK version string."""
    ptr = library().ilink_version()
    if not ptr:
        raise IlinkError("ilink_version returned NULL")
    return _take_string(ptr)


def call(op: str, params: Optional[Mapping[str, Any]] = None) -> Any:
    """Run one SDK operation and return its decoded ``result``.

    >>> call("mime.info", {"path": "photo.png"})["mime"]
    'image/png'
    """
    lib = library()
    params_json = None if params is None else json.dumps(params)
    raw = lib.ilink_call(
        op.encode("utf-8"),
        None if params_json is None else params_json.encode("utf-8"),
    )
    if not raw:
        raise IlinkError("ilink_call returned NULL")

    envelope = json.loads(_take_string(raw))
    if envelope.get("ok"):
        return envelope.get("result")
    raise IlinkError(envelope.get("error", "operation failed"), envelope.get("kind", "unknown"))
