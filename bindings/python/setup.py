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
import re
import shutil
import struct
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

#: ELF ``e_machine`` -> wheel architecture.
_ELF_MACHINE = {
    0x03: "i686",
    0x28: "armv7l",
    0x3E: "x86_64",
    0xB7: "aarch64",
    0xF3: "riscv64",
}

#: Mach-O ``cputype`` -> wheel architecture.
_MACHO_CPU = {
    7: "i386",
    12: "armv7l",
    0x01000007: "x86_64",
    0x0100000C: "arm64",
}

#: PE ``Machine`` -> wheel platform.
_PE_MACHINE = {
    0x014C: "win32",
    0x8664: "win_amd64",
    0xAA64: "win_arm64",
}

#: Platform tag used when the bundled library cannot be inspected.
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


# ---------------------------------------------------------------------------
# Platform tagging
#
# The wheel tag must describe the *native library that is bundled*, not the
# build host. Deriving it from the host gets two things wrong:
#
#   * Linux wheels get the generic ``linux_x86_64`` tag, which PyPI rejects:
#     it wants a PEP 600 ``manylinux_x_y`` tag that states the glibc floor.
#   * macOS runners ship a universal2 Python, so ``bdist_wheel`` reports
#     ``macosx_*_universal2`` even when the bundled dylib is a thin
#     single-architecture build. Claiming universal2 would let pip install an
#     x86_64-only dylib on Apple Silicon.
#
# So inspect the library itself and describe what it really is.
# ---------------------------------------------------------------------------


def _max_glibc(data: bytes) -> tuple[int, int] | None:
    """The highest ``GLIBC_x.y`` version referenced by an ELF image."""
    best: tuple[int, int] | None = None
    for match in re.finditer(rb"GLIBC_(\d+)\.(\d+)", data):
        version = (int(match.group(1)), int(match.group(2)))
        if best is None or version > best:
            best = version
    return best


def _elf_platform(data: bytes) -> str:
    machine = struct.unpack_from("<H", data, 18)[0]
    arch = _ELF_MACHINE.get(machine, "x86_64")
    glibc = _max_glibc(data)
    if glibc is None:
        # Not linked against glibc (musl, or a static build): there is no
        # manylinux floor to promise, so stay with the generic tag.
        return f"linux_{arch}"
    return f"manylinux_{glibc[0]}_{glibc[1]}_{arch}"


def _macho_slices(data: bytes) -> list[tuple[int, tuple[int, int]]]:
    """``(cputype, (major, minor))`` for each architecture in a Mach-O image."""
    magic = struct.unpack_from(">I", data, 0)[0]
    offsets: list[int] = []
    if magic in (0xCAFEBABE, 0xCAFEBABF):  # fat binary
        count = struct.unpack_from(">I", data, 4)[0]
        for index in range(count):
            offsets.append(struct.unpack_from(">I", data, 8 + 20 * index + 8)[0])
    else:
        offsets.append(0)

    slices: list[tuple[int, tuple[int, int]]] = []
    for offset in offsets:
        cpu = struct.unpack_from("<i", data, offset + 4)[0] & 0xFFFFFFFF
        ncmds = struct.unpack_from("<I", data, offset + 16)[0]
        cursor = offset + 32
        minimum = (10, 12)
        for _ in range(ncmds):
            cmd, size = struct.unpack_from("<II", data, cursor)
            if cmd == 0x32:  # LC_BUILD_VERSION
                raw = struct.unpack_from("<I", data, cursor + 12)[0]
                minimum = (raw >> 16, (raw >> 8) & 0xFF)
                break
            if cmd == 0x24:  # LC_VERSION_MIN_MACOSX
                raw = struct.unpack_from("<I", data, cursor + 8)[0]
                minimum = (raw >> 16, (raw >> 8) & 0xFF)
                break
            cursor += size
        slices.append((cpu, minimum))
    return slices


def _macho_platform(data: bytes) -> str:
    slices = _macho_slices(data)
    arches = {_MACHO_CPU.get(cpu, "x86_64") for cpu, _ in slices}
    # A genuine universal2 build can run on both Intel and Apple Silicon.
    if arches == {"x86_64", "arm64"}:
        arch = "universal2"
        # universal2 requires a deployment target of 11.0 or newer.
        minimum = max(minimum for _, minimum in slices)
        minimum = max(minimum, (11, 0))
    else:
        arch = next(iter(arches))
        minimum = slices[0][1]
        if arch == "arm64":
            minimum = max(minimum, (11, 0))
    return f"macosx_{minimum[0]}_{minimum[1]}_{arch}"


def _pe_platform(data: bytes) -> str:
    offset = struct.unpack_from("<I", data, 0x3C)[0]
    machine = struct.unpack_from("<H", data, offset + 4)[0]
    return _PE_MACHINE.get(machine, "win_amd64")


def platform_tag(library: Path) -> str:
    """The wheel platform tag describing ``library``."""
    data = library.read_bytes()
    if data[:4] == b"\x7fELF":
        return _elf_platform(data)
    if data[:2] == b"MZ":
        return _pe_platform(data)
    if struct.unpack_from(">I", data, 0)[0] in (0xCAFEBABE, 0xCAFEBABF) or \
            struct.unpack_from("<I", data, 0)[0] in (0xFEEDFACE, 0xFEEDFACF):
        return _macho_platform(data)
    raise SystemExit(f"unrecognised binary format in {library}")


def native_platform_tag() -> str:
    """Fallback tag for the host, used when the library cannot be inspected."""
    try:
        return platform_tag(find_library())
    except SystemExit:
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
        """Mark the wheel as platform-specific and tag it from the library."""

        def finalize_options(self) -> None:
            super().finalize_options()
            self.root_is_pure = False

        def get_tag(self):  # type: ignore[override]
            _python, _abi, _plat = super().get_tag()
            # Ignore the platform tag setuptools derived from the *host*: on
            # macOS that is the universal2 tag of the runner's Python, which
            # would misdescribe a thin dylib. Describe the bundled library
            # instead. ctypes works on any CPython 3.x, so the wheel is
            # py3-none-<plat>.
            return "py3", "none", native_platform_tag()

else:  # pragma: no cover
    bdist_wheel = None


setup(cmdclass={"build_py": build_py, **({"bdist_wheel": bdist_wheel} if bdist_wheel else {})})
