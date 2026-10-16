"""Build the ``ilink-sdk`` Python wheel with the native library bundled in.

The Python bindings are a ``ctypes`` wrapper around the Rust library
(``ilink_sdk.dll`` / ``libilink_sdk.so`` / ``libilink_sdk.dylib``). For the
wheel to work out of the box, the native library must be copied *into* the
package, otherwise ``pip install ilink-sdk`` produces an importable package that
fails at the first call.

The library is located (in order) via:

1. ``ILINK_SDK_LIB`` — full path to the shared library,
2. ``target/release/`` of a source checkout (the repository root).

``python -m build`` / ``pip wheel`` then copies it into ``ilink_sdk/`` and tags
the wheel for the current platform (it is **not** a pure-Python wheel). CI runs
this once per OS/arch to publish platform wheels.
"""

from __future__ import annotations

import os
import shutil
import sys
from pathlib import Path

from setuptools import setup
from setuptools.command.build_py import build_py as _build_py

try:  # setuptools ships a vendored copy; wheel may also provide one.
    from wheel.bdist_wheel import bdist_wheel as _bdist_wheel
except ImportError:  # pragma: no cover - wheel is a build requirement
    _bdist_wheel = None  # type: ignore[assignment]


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parent.parent

#: Platform tag used when the host is not recognised by ``bdist_wheel``.
_FALLBACK_PLAT = {
    "win32": "win_amd64",
    "darwin": "macosx_11_0_arm64",
    "linux": "manylinux2014_x86_64",
}


def library_filename() -> str:
    """The native library file name for the current platform."""
    if sys.platform == "win32":
        return "ilink_sdk.dll"
    if sys.platform == "darwin":
        return "libilink_sdk.dylib"
    return "libilink_sdk.so"


def find_library() -> Path:
    """Locate the native library to bundle, or raise a helpful error."""
    override = os.environ.get("ILINK_SDK_LIB")
    if override:
        path = Path(override).expanduser().resolve()
        if path.is_file():
            return path
        raise SystemExit(f"ILINK_SDK_LIB points at a missing file: {path}")

    name = library_filename()
    candidates = [
        REPO_ROOT / "target" / "release" / name,
        HERE / "ilink_sdk" / name,
    ]
    for candidate in candidates:
        if candidate.is_file():
            return candidate

    raise SystemExit(
        f"could not find {name}; build it with `cargo build --release` from the "
        f"repository root, or set ILINK_SDK_LIB to its full path"
    )


def native_platform_tag() -> str:
    """The wheel platform tag for the host, e.g. ``win_amd64``."""
    if sys.platform == "win32":
        import platform

        machine = platform.machine().lower()
        return "win_arm64" if machine in ("arm64", "aarch64") else "win_amd64"
    if sys.platform == "darwin":
        import platform

        machine = platform.machine().lower()
        return "macosx_11_0_arm64" if machine in ("arm64", "aarch64") else "macosx_10_12_x86_64"
    # Linux: the CI builds inside a manylinux container, so the tag is already
    # correct; a plain source build still needs *a* platform tag.
    return _FALLBACK_PLAT.get(sys.platform, "manylinux2014_x86_64")


class build_py(_build_py):
    """Copy the native library into the built package."""

    def run(self) -> None:
        super().run()
        self._bundle_native_library()

    def _bundle_native_library(self) -> None:
        source = find_library()
        target = Path(self.build_lib) / "ilink_sdk" / source.name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
        # The library is loaded by ctypes, not imported, but keeping it
        # executable avoids surprises on some loaders.
        target.chmod(target.stat().st_mode | 0o755)
        print(f"bundled native library: {source} -> {target}")


if _bdist_wheel is not None:

    class bdist_wheel(_bdist_wheel):  # type: ignore[misc, valid-type]
        """Mark the wheel as platform-specific and tag it for the host."""

        def finalize_options(self) -> None:
            super().finalize_options()
            self.root_is_pure = False
            if self.plat_name is None:
                self.plat_name = native_platform_tag()

        def get_tag(self):  # type: ignore[override]
            _python, _abi, plat = super().get_tag()
            # ctypes works on any CPython 3.x, so the wheel is py3-none-<plat>.
            return "py3", "none", plat

else:  # pragma: no cover
    bdist_wheel = None


setup(cmdclass={"build_py": build_py, **({"bdist_wheel": bdist_wheel} if bdist_wheel else {})})
