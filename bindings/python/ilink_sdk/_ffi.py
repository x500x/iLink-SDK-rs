"""ctypes loader for the iLink SDK shared library.

The Rust crate builds `ilink_sdk.dll` / `libilink_sdk.so` / `libilink_sdk.dylib`
with a single JSON-in / JSON-out C ABI (see `include/ilink_sdk.h`). This module
finds and loads that library once, and exposes thin wrappers around the three
exported symbols.
"""

from __future__ import annotations

import ctypes
import ctypes.util
import os
import sys
from typing import Optional

_LIB_ENV = "ILINK_SDK_LIB"


def _library_filenames() -> list[str]:
    if sys.platform == "win32":
        return ["ilink_sdk.dll"]
    if sys.platform == "darwin":
        return ["libilink_sdk.dylib"]
    return ["libilink_sdk.so"]


def _candidate_paths() -> list[str]:
    candidates: list[str] = []
    override = os.environ.get(_LIB_ENV)
    if override:
        candidates.append(override)

    names = _library_filenames()
    here = os.path.dirname(os.path.abspath(__file__))
    # 1. next to this package (installed wheel / copied library)
    # 2. the repository's cargo output (development checkout)
    roots = [
        here,
        os.path.join(here, "..", "..", "..", "target", "release"),
        os.path.join(os.getcwd(), "target", "release"),
    ]
    for root in roots:
        for name in names:
            candidates.append(os.path.normpath(os.path.join(root, name)))

    # 3. whatever the OS loader knows about
    found = ctypes.util.find_library("ilink_sdk")
    if found:
        candidates.append(found)
    candidates.extend(names)
    return candidates


_lib: Optional[ctypes.CDLL] = None


def library() -> ctypes.CDLL:
    """Load (once) and return the shared library."""
    global _lib
    if _lib is not None:
        return _lib

    errors: list[str] = []
    for path in _candidate_paths():
        try:
            lib = ctypes.CDLL(path)
        except OSError as exc:  # pragma: no cover - depends on host layout
            errors.append(f"{path}: {exc}")
            continue
        lib.ilink_call.argtypes = [ctypes.c_char_p, ctypes.c_char_p]
        lib.ilink_call.restype = ctypes.c_void_p
        lib.ilink_version.argtypes = []
        lib.ilink_version.restype = ctypes.c_void_p
        lib.ilink_string_free.argtypes = [ctypes.c_void_p]
        lib.ilink_string_free.restype = None
        _lib = lib
        return lib

    tried = "\n  ".join(errors) if errors else "(no candidates)"
    raise OSError(
        f"could not load the iLink SDK library; set {_LIB_ENV} to its full path.\n"
        f"Tried:\n  {tried}"
    )
