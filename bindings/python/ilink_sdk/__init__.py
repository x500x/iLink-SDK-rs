"""Python bindings for the iLink (Weixin) SDK.

The bindings are a dependency-free (``ctypes``) wrapper around the Rust
library's JSON C ABI. The top-level :func:`call` returns decoded Python data,
while :class:`Client` returns typed :mod:`ilink_sdk.models` objects and carries
default routing parameters for every call:

>>> import ilink_sdk
>>> ilink_sdk.version()
'2.4.10'
>>> ilink_sdk.call("mime.info", {"path": "photo.png"})["mime"]
'image/png'
>>> client = ilink_sdk.Client(account_id="cfe9a2017e77-im-bot")
>>> client.send_text("someone@im.wechat", "hi").message_id
'...'

Set ``ILINK_SDK_LIB`` to the full path of ``ilink_sdk.dll`` /
``libilink_sdk.so`` / ``libilink_sdk.dylib`` if it is not on the loader path.
"""

from . import models
from ._call import IlinkError, call, version
from ._ffi import library
from .client import Client
from .models import (
    Account,
    InboundMedia,
    LoginResult,
    LoginStart,
    MessageItem,
    MsgContext,
    SendResult,
    Updates,
    UploadedFile,
    WeixinMessage,
)

__all__ = [
    "Account",
    "Client",
    "IlinkError",
    "InboundMedia",
    "LoginResult",
    "LoginStart",
    "MessageItem",
    "MsgContext",
    "SendResult",
    "Updates",
    "UploadedFile",
    "WeixinMessage",
    "call",
    "library",
    "models",
    "version",
]
__version__ = "2.4.10"
