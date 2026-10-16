#!/usr/bin/env python3
"""Stage a built native library into a binding's resources for packaging.

The JVM bindings (Java, Kotlin) bundle one native library per platform inside
their JAR, under ``io/ilink/sdk/native/<platform>/``. The loader extracts the
entry for the running OS/CPU at runtime, so a published JAR works everywhere
without ``-Djna.library.path``.

Usage::

    # stage target/release/ilink_sdk.dll for the current host
    python bindings/scripts/stage_native.py --binding java

    # stage a cross-built library for an explicit platform
    python bindings/scripts/stage_native.py --binding kotlin \
        --platform linux-aarch64 --library path/to/libilink_sdk.so

The platform name matches the release artifact names:
``linux-x86_64``, ``linux-aarch64``, ``macos-x86_64``, ``macos-aarch64``,
``windows-x86_64``, ``windows-aarch64``.
"""

from __future__ import annotations

import argparse
import os
import platform
import shutil
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

PLATFORMS = {
    "linux-x86_64": "libilink_sdk.so",
    "linux-aarch64": "libilink_sdk.so",
    "macos-x86_64": "libilink_sdk.dylib",
    "macos-aarch64": "libilink_sdk.dylib",
    "windows-x86_64": "ilink_sdk.dll",
    "windows-aarch64": "ilink_sdk.dll",
}

#: Where each binding keeps its JVM resources.
RESOURCE_ROOTS = {
    "java": "bindings/java/src/main/resources",
    "kotlin": "bindings/kotlin/src/main/resources",
    "go": "bindings/go/ilink/native",
}

RESOURCE_PREFIX = "io/ilink/sdk/native"

#: Bindings whose resources are rooted at the resource dir itself (no prefix).
FLAT_ROOTS = {"go"}


def host_platform() -> str:
    system = platform.system().lower()
    machine = platform.machine().lower()
    aarch64 = machine in ("arm64", "aarch64")
    if system == "windows":
        return "windows-aarch64" if aarch64 else "windows-x86_64"
    if system == "darwin":
        return "macos-aarch64" if aarch64 else "macos-x86_64"
    return "linux-aarch64" if aarch64 else "linux-x86_64"


def find_library(explicit: str | None) -> Path:
    if explicit:
        path = Path(explicit).expanduser().resolve()
        if not path.is_file():
            raise SystemExit(f"--library does not exist: {path}")
        return path

    override = os.environ.get("ILINK_SDK_LIB")
    if override:
        path = Path(override).expanduser().resolve()
        if path.is_file():
            return path

    # Current-platform file name, searched in the usual build output dirs.
    name = PLATFORMS[host_platform()]
    for candidate in [
        REPO_ROOT / "target" / "release" / name,
        REPO_ROOT / "target" / "debug" / name,
    ]:
        if candidate.is_file():
            return candidate

    raise SystemExit(
        f"could not find {name}; build it with `cargo build --release`, pass "
        f"--library, or set ILINK_SDK_LIB"
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binding",
        required=True,
        choices=sorted(RESOURCE_ROOTS),
        help="which binding's resources to stage into",
    )
    parser.add_argument(
        "--platform",
        default=None,
        choices=sorted(PLATFORMS),
        help="platform directory name (defaults to the current host)",
    )
    parser.add_argument(
        "--library",
        default=None,
        help="path to the native library (defaults to target/release or $ILINK_SDK_LIB)",
    )
    parser.add_argument(
        "--clean",
        action="store_true",
        help="remove previously staged libraries for this binding first",
    )
    args = parser.parse_args()

    platform_name = args.platform or host_platform()
    root = REPO_ROOT / RESOURCE_ROOTS[args.binding]
    if args.binding in FLAT_ROOTS:
        target_dir = root / platform_name
        clean_root = root
    else:
        target_dir = root / RESOURCE_PREFIX / platform_name
        clean_root = root / RESOURCE_PREFIX

    if args.clean:
        if clean_root.exists():
            # Only remove staged platform directories, never the placeholder
            # README that keeps the embed/resource directory in git.
            for child in clean_root.iterdir():
                if child.is_dir() and child.name in PLATFORMS:
                    shutil.rmtree(child)
                    print(f"cleaned {child}")

    source = find_library(args.library)
    target_dir.mkdir(parents=True, exist_ok=True)
    target = target_dir / source.name
    shutil.copy2(source, target)
    print(f"staged {source} -> {target}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
