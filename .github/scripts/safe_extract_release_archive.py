#!/usr/bin/env python3
"""Extract a release archive without accepting traversal or link entries."""

from __future__ import annotations

import argparse
import shutil
import tarfile
from pathlib import Path, PurePosixPath


class ArchiveSafetyError(RuntimeError):
    pass


def safe_member_path(name: str) -> Path:
    path = PurePosixPath(name)
    if path.is_absolute() or not path.parts or ".." in path.parts or "\\" in name:
        raise ArchiveSafetyError(f"archive member path is unsafe: {name!r}")
    return Path(*path.parts)


def extract_archive(archive_path: Path, destination: Path) -> None:
    destination = destination.resolve()
    destination.mkdir(parents=True, exist_ok=True)
    if any(destination.iterdir()):
        raise ArchiveSafetyError(f"destination must be empty: {destination}")
    seen: set[Path] = set()
    with tarfile.open(archive_path, mode="r:gz") as archive:
        for member in archive:
            relative = safe_member_path(member.name)
            if relative in seen:
                raise ArchiveSafetyError(f"archive contains duplicate member: {member.name!r}")
            seen.add(relative)
            target = (destination / relative).resolve()
            if destination not in target.parents:
                raise ArchiveSafetyError(f"archive member escapes destination: {member.name!r}")
            if member.issym() or member.islnk() or not (member.isdir() or member.isreg()):
                raise ArchiveSafetyError(f"archive member type is unsafe: {member.name!r}")
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                target.chmod(0o755)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            source = archive.extractfile(member)
            if source is None:
                raise ArchiveSafetyError(f"archive member has no readable content: {member.name!r}")
            with source, target.open("wb") as output:
                shutil.copyfileobj(source, output)
            target.chmod(0o755 if member.mode & 0o111 else 0o644)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args()
    try:
        extract_archive(args.archive, args.destination)
    except (ArchiveSafetyError, OSError, tarfile.TarError) as error:
        print(f"safe_extract_release_archive.py: {error}")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
