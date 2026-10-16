"""Python bindings for the iLink (Weixin) SDK.

The bindings are a thin, dependency-free (``ctypes``) wrapper around the Rust
library's JSON C ABI:

>>> import ilink_sdk
>>> ilink_sdk.version()
'2.4.10'
>>> ilink_sdk.call("mime.info", {"path": "photo.png"})["mime"]
'image/png'

Set ``ILINK_SDK_LIB`` to the full path of ``ilink_sdk.dll`` /
``libilink_sdk.so`` / ``libilink_sdk.dylib`` if it is not on the loader path.
"""

from ._call import IlinkError, call, version
from ._ffi import library
from .client import Client

__all__ = ["Client", "IlinkError", "call", "library", "version"]
__version__ = "2.4.10"
