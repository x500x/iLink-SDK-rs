#!/usr/bin/env python3
"""Synchronise the version across the crate and every language binding.

The version lives in several files (Cargo.toml, pyproject.toml, pom.xml,
build.gradle.kts, ...). Release automation derives it from the git tag and calls
this script so they never drift apart.

Usage::

    # use the version from Cargo.toml as the source of truth
    python bindings/scripts/set_version.py

    # or set an explicit version (this is what CI does, from the tag)
    python bindings/scripts/set_version.py 2.5.0

Pass ``--check`` to only verify that everything matches (exit 1 otherwise),
which is handy in CI and pre-commit hooks.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

#: Files that carry the version, with a regex whose group 1 is the version.
TARGETS = {
    "Cargo.toml": re.compile(r'^version\s*=\s*"([^"]+)"', re.MULTILINE),
    "bindings/python/pyproject.toml": re.compile(r'^version\s*=\s*"([^"]+)"', re.MULTILINE),
    "bindings/python/ilink_sdk/__init__.py": re.compile(
        r'^__version__\s*=\s*"([^"]+)"', re.MULTILINE
    ),
    "bindings/java/pom.xml": re.compile(r"<version>([^<]+)</version>", re.MULTILINE),
    "bindings/kotlin/build.gradle.kts": re.compile(r'^version\s*=\s*"([^"]+)"', re.MULTILINE),
    "bindings/go/ilink/version.go": re.compile(r'sdkVersion\s*=\s*"([^"]+)"'),
    # Version strings shown in the per-language READMEs (dependency snippets).
    "bindings/java/README.md": re.compile(
        r"<artifactId>ilink-sdk</artifactId>\n\s*<version>([^<]+)</version>"
    ),
    "bindings/java/README.zh.md": re.compile(
        r"<artifactId>ilink-sdk</artifactId>\n\s*<version>([^<]+)</version>"
    ),
    "bindings/kotlin/README.md": re.compile(r'ilink-sdk-kotlin:([0-9][^"]*)"'),
    "bindings/kotlin/README.zh.md": re.compile(r'ilink-sdk-kotlin:([0-9][^"]*)"'),
}


def read_versions() -> dict[str, str]:
    """Return the version currently declared in each file."""
    found: dict[str, str] = {}
    for relative, pattern in TARGETS.items():
        path = REPO_ROOT / relative
        if not path.is_file():
            raise SystemExit(f"missing file: {relative}")
        match = pattern.search(path.read_text(encoding="utf-8"))
        if not match:
            raise SystemExit(f"could not find a version in {relative}")
        found[relative] = match.group(1)
    return found


def replace_version(relative: str, pattern: re.Pattern[str], version: str) -> None:
    path = REPO_ROOT / relative
    text = path.read_text(encoding="utf-8")
    new_text, count = pattern.subn(
        lambda m: m.group(0).replace(m.group(1), version), text, count=1
    )
    if count == 0:
        raise SystemExit(f"could not update {relative}")
    if new_text != text:
        path.write_text(new_text, encoding="utf-8")
        print(f"updated {relative} -> {version}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "version",
        nargs="?",
        help="the version to set (defaults to the version in Cargo.toml)",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify that all files agree instead of writing them",
    )
    args = parser.parse_args()

    current = read_versions()

    if args.check:
        unique = set(current.values())
        if len(unique) == 1:
            print(f"all {len(current)} files agree on {unique.pop()}")
            return 0
        print("version mismatch:", file=sys.stderr)
        for relative, version in sorted(current.items()):
            print(f"  {version:<12} {relative}", file=sys.stderr)
        return 1

    version = args.version or current["Cargo.toml"]
    if not re.fullmatch(r"\d+\.\d+\.\d+([-.+][0-9A-Za-z.-]+)?", version):
        raise SystemExit(f"not a valid semver version: {version!r}")

    for relative, pattern in TARGETS.items():
        replace_version(relative, pattern, version)

    print(f"version set to {version}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
