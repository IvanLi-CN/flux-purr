#!/usr/bin/env python3
"""Create a reproducible gzip-compressed tar archive for release assets."""

from __future__ import annotations

import argparse
import gzip
import os
import tarfile
import tempfile
from pathlib import Path


def iter_paths(root: Path, requested: list[str]) -> list[tuple[Path, str]]:
    selected = [root / relative for relative in requested] if requested else sorted(root.iterdir())
    result: list[tuple[Path, str]] = []

    def visit(path: Path, relative: Path) -> None:
        if path.is_symlink():
            raise ValueError(f"archive does not allow symlinks: {relative}")
        if not path.is_file() and not path.is_dir():
            raise ValueError(f"archive only supports regular files and directories: {relative}")
        result.append((path, relative.as_posix()))
        if path.is_dir():
            for child in sorted(path.iterdir(), key=lambda item: item.name):
                visit(child, relative / child.name)

    for path, relative in zip(selected, requested or [item.name for item in selected]):
        relative_path = Path(relative)
        if relative_path.is_absolute() or ".." in relative_path.parts:
            raise ValueError(f"archive path escapes source root: {relative}")
        if not path.exists() and not path.is_symlink():
            raise FileNotFoundError(path)
        visit(path, relative_path)
    return result


def create_archive(source_root: Path, output: Path, requested: list[str]) -> None:
    source_root = source_root.resolve()
    output = output.resolve()
    if not source_root.is_dir():
        raise NotADirectoryError(source_root)
    output.parent.mkdir(parents=True, exist_ok=True)
    entries = iter_paths(source_root, requested)
    fd, temporary_name = tempfile.mkstemp(prefix=f".{output.name}.", dir=output.parent)
    os.close(fd)
    temporary = Path(temporary_name)
    try:
        with temporary.open("wb") as raw_output:
            with gzip.GzipFile(fileobj=raw_output, mode="wb", filename="", mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as archive:
                    for path, relative in entries:
                        info = archive.gettarinfo(str(path), arcname=relative)
                        info.uid = 0
                        info.gid = 0
                        info.uname = ""
                        info.gname = ""
                        info.mtime = 0
                        info.mode = 0o755 if info.isdir() or (info.mode & 0o111) else 0o644
                        info.pax_headers = {}
                        if info.isreg():
                            with path.open("rb") as source:
                                archive.addfile(info, source)
                        else:
                            archive.addfile(info)
        os.replace(temporary, output)
    finally:
        if temporary.exists():
            temporary.unlink()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--path", action="append", default=[])
    args = parser.parse_args()
    create_archive(args.source_root, args.output, args.path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
