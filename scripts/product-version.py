#!/usr/bin/env python3
"""Resolve the Flux Purr product build identity from the root VERSION file."""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Final


VERSION_RE: Final = re.compile(
    r"^(?P<major>0|[1-9][0-9]*)\.(?P<minor>0|[1-9][0-9]*)\.(?P<patch>0|[1-9][0-9]*)(?P<prerelease>-rc\.(?P<rc>[1-9][0-9]*))?$"
)
SOURCE_SHA_RE: Final = re.compile(r"^[0-9a-f]{40}$")
BUILD_ID_RE: Final = re.compile(r"^[0-9a-f]{16,64}$")


class VersionError(ValueError):
    """Raised when VERSION or build identity inputs are invalid."""


def parse_version(value: str) -> dict[str, int | str | None]:
    match = VERSION_RE.fullmatch(value)
    if match is None:
        raise VersionError(f"invalid product VERSION: {value!r}")
    return {
        "major": int(match.group("major")),
        "minor": int(match.group("minor")),
        "patch": int(match.group("patch")),
        "prerelease": match.group("prerelease"),
        "rc": int(match.group("rc")) if match.group("rc") else None,
    }


def read_version(version_file: Path) -> str:
    try:
        text = version_file.read_text(encoding="utf-8")
    except OSError as error:
        raise VersionError(f"cannot read product VERSION at {version_file}: {error}") from error
    if not text.endswith("\n") or text.endswith("\n\n"):
        raise VersionError("VERSION must contain exactly one trailing LF")
    value = text[:-1]
    if "\n" in value or value != value.strip():
        raise VersionError("VERSION must contain exactly one non-blank line")
    parse_version(value)
    return value


def read_version_from_text(text: str) -> str:
    if not text.endswith("\n") or text.endswith("\n\n"):
        raise VersionError("VERSION must contain exactly one trailing LF")
    value = text[:-1]
    if "\n" in value or value != value.strip():
        raise VersionError("VERSION must contain exactly one non-blank line")
    parse_version(value)
    return value


def next_patch(value: str) -> str:
    parsed = parse_version(value)
    return f"{parsed['major']}.{parsed['minor']}.{int(parsed['patch']) + 1}"


def channel_for(mode: str, version: str) -> str:
    if mode == "development":
        return "local"
    return "rc" if parse_version(version)["prerelease"] else "stable"


def source_sha(repo_root: Path, supplied: str | None) -> str:
    value = supplied or os.environ.get("FLUX_PURR_SOURCE_SHA")
    if not value:
        try:
            value = subprocess.check_output(
                ["git", "-C", str(repo_root), "rev-parse", "HEAD"],
                text=True,
                stderr=subprocess.PIPE,
            ).strip()
        except (OSError, subprocess.CalledProcessError) as error:
            raise VersionError("source SHA is required when the repository is not a Git checkout") from error
    if not SOURCE_SHA_RE.fullmatch(value):
        raise VersionError("source SHA must be a 40-character lowercase hexadecimal commit SHA")
    return value


def resolve(repo_root: Path, mode: str, supplied_source_sha: str | None = None) -> dict[str, str]:
    version = read_version(repo_root / "VERSION")
    if mode == "development":
        source = source_sha(repo_root, supplied_source_sha)
        product_version = f"{next_patch(version)}-dev.{source[:7]}"
    elif mode == "release":
        source = source_sha(repo_root, supplied_source_sha)
        product_version = version
    elif mode == "next-patch":
        source = source_sha(repo_root, supplied_source_sha)
        product_version = next_patch(version)
    else:
        raise VersionError(f"unsupported build mode: {mode}")
    build_id = source[:16]
    if not BUILD_ID_RE.fullmatch(build_id):
        raise VersionError("derived build ID is invalid")
    return {
        "version": product_version,
        "channel": channel_for("development" if mode == "development" else "release", product_version),
        "sourceSha": source,
        "buildId": build_id,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--mode", choices=("development", "release", "next-patch"), required=True)
    parser.add_argument("--source-sha")
    parser.add_argument("--format", choices=("plain", "json", "tsv"), default="plain")
    args = parser.parse_args(argv)
    try:
        identity = resolve(args.repo_root.resolve(), args.mode, args.source_sha)
    except VersionError as error:
        print(f"product-version: {error}", file=sys.stderr)
        return 1
    if args.format == "plain":
        print(identity["version"])
    elif args.format == "json":
        print(json.dumps(identity, sort_keys=True, separators=(",", ":")))
    else:
        print("\t".join(identity[key] for key in ("version", "channel", "sourceSha", "buildId")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
