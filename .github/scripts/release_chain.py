#!/usr/bin/env python3
"""Validate and stage Flux Purr one-source-commit release chains."""

from __future__ import annotations

import argparse
import importlib.util
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
RESOLVER_PATH = ROOT / "scripts/product-version.py"
SPEC = importlib.util.spec_from_file_location("product_version", RESOLVER_PATH)
if SPEC is None or SPEC.loader is None:
    raise RuntimeError(f"unable to load {RESOLVER_PATH}")
PRODUCT_VERSION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PRODUCT_VERSION)


class ReleaseChainError(RuntimeError):
    pass


def git(*args: str, check: bool = True) -> str:
    result = subprocess.run(["git", *args], cwd=ROOT, text=True, capture_output=True)
    if check and result.returncode != 0:
        raise ReleaseChainError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result.stdout.strip()


def git_raw(*args: str, check: bool = True) -> str:
    result = subprocess.run(["git", *args], cwd=ROOT, text=True, capture_output=True)
    if check and result.returncode != 0:
        raise ReleaseChainError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result.stdout


def commit_parent(commit: str) -> str:
    parents = git("show", "-s", "--format=%P", commit).split()
    if len(parents) != 1:
        raise ReleaseChainError(f"Release Commit {commit} must have exactly one parent")
    return parents[0]


def commit_version(commit: str) -> str:
    contents = git_raw("show", f"{commit}:VERSION")
    if not contents.endswith("\n"):
        raise ReleaseChainError(f"{commit}:VERSION must end with one LF")
    return PRODUCT_VERSION.read_version_from_text(contents)


def diff_names(commit: str) -> list[str]:
    parent = commit_parent(commit)
    return git("diff", "--name-only", f"{parent}..{commit}").splitlines()


def trailers(commit: str) -> dict[str, str]:
    raw = git("show", "-s", "--format=%(trailers:only,unfold,separator=%x00)", commit)
    values: dict[str, str] = {}
    for item in raw.split("\x00"):
        if ":" not in item:
            continue
        key, value = item.split(":", 1)
        values[key.strip()] = value.strip()
    return values


def verify_release_commit(commit: str, source_sha: str | None = None, expected_version: str | None = None) -> dict[str, str]:
    commit = git("rev-parse", f"{commit}^{{commit}}")
    parent = commit_parent(commit)
    if source_sha and parent != source_sha:
        raise ReleaseChainError(f"Release Commit {commit} parent is {parent}, expected {source_sha}")
    names = diff_names(commit)
    if names != ["VERSION"]:
        raise ReleaseChainError(f"Release Commit {commit} must change only VERSION, got {names}")
    version = commit_version(commit)
    if expected_version and version != expected_version:
        raise ReleaseChainError(f"Release Commit VERSION is {version}, expected {expected_version}")
    commit_trailers = trailers(commit)
    if commit_trailers.get("Release-Source-SHA") != parent:
        raise ReleaseChainError("Release Commit is missing a matching Release-Source-SHA trailer")
    if commit_trailers.get("Product-Version") != version:
        raise ReleaseChainError("Release Commit is missing a matching Product-Version trailer")
    return {"releaseSha": commit, "sourceSha": parent, "version": version, "tag": f"v{version}"}


def write_github_output(values: dict[str, str], path: str | None) -> None:
    if not path:
        return
    with Path(path).open("a", encoding="utf-8") as output:
        for key, value in values.items():
            output.write(f"{key}={value}\n")


def stage(args: argparse.Namespace) -> None:
    source_sha = git("rev-parse", "HEAD")
    if source_sha != args.source_sha:
        raise ReleaseChainError(f"checked out source is {source_sha}, expected {args.source_sha}")
    if git("status", "--porcelain"):
        raise ReleaseChainError("source checkout must be clean before staging a Release Commit")
    try:
        verify_release_commit(source_sha)
    except ReleaseChainError:
        pass
    else:
        raise ReleaseChainError(f"source {source_sha} is already a Release Commit")
    current = PRODUCT_VERSION.read_version(ROOT / "VERSION")
    if args.mode == "automatic":
        version = PRODUCT_VERSION.next_patch(current)
    else:
        if not args.exact_version:
            raise ReleaseChainError("exact mode requires --exact-version")
        PRODUCT_VERSION.parse_version(args.exact_version)
        version = args.exact_version
    (ROOT / "VERSION").write_text(version + "\n", encoding="utf-8")
    if git("diff", "--name-only") != "VERSION":
        raise ReleaseChainError("staging a Release Commit may modify only VERSION")
    subprocess.run(
        [
            "git",
            "add",
            "VERSION",
        ],
        cwd=ROOT,
        check=True,
    )
    subprocess.run(
        [
            "git",
            "commit",
            "--signoff",
            "-m",
            f"chore(release): v{version}",
            "-m",
            f"Release-Source-SHA: {source_sha}\nProduct-Version: {version}",
        ],
        cwd=ROOT,
        check=True,
    )
    release_sha = git("rev-parse", "HEAD")
    values = verify_release_commit(release_sha, source_sha, version)
    write_github_output(values, args.github_output)
    print(json.dumps(values, sort_keys=True))


def promote(args: argparse.Namespace) -> None:
    release = verify_release_commit(args.commit)
    checked_out = git("rev-parse", "HEAD")
    if checked_out != release["releaseSha"]:
        raise ReleaseChainError(
            f"promotion checkout is {checked_out}, expected RC Release Commit {release['releaseSha']}"
        )
    parsed = PRODUCT_VERSION.parse_version(release["version"])
    if not parsed["prerelease"]:
        raise ReleaseChainError("promotion requires an RC Release Commit")
    stable_version = f"{parsed['major']}.{parsed['minor']}.{parsed['patch']}"
    if args.exact_version and args.exact_version != stable_version:
        raise ReleaseChainError("promotion version must remove only the RC prerelease")
    (ROOT / "VERSION").write_text(stable_version + "\n", encoding="utf-8")
    if git("diff", "--name-only") != "VERSION":
        raise ReleaseChainError("promotion Release Commit may modify only VERSION")
    subprocess.run(["git", "add", "VERSION"], cwd=ROOT, check=True)
    subprocess.run(
        [
            "git",
            "commit",
            "--signoff",
            "-m",
            f"chore(release): v{stable_version}",
            "-m",
            f"Release-Source-SHA: {release['releaseSha']}\nProduct-Version: {stable_version}",
        ],
        cwd=ROOT,
        check=True,
    )
    values = verify_release_commit(git("rev-parse", "HEAD"), release["releaseSha"], stable_version)
    write_github_output(values, args.github_output)
    print(json.dumps(values, sort_keys=True))


def check_commit(args: argparse.Namespace) -> None:
    commit = args.commit or git("rev-parse", "HEAD")
    values = verify_release_commit(commit, args.source_sha, args.version)
    write_github_output(values, args.github_output)
    print(json.dumps(values, sort_keys=True))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    stage_parser = sub.add_parser("stage")
    stage_parser.add_argument("--source-sha", required=True)
    stage_parser.add_argument("--mode", choices=("automatic", "exact"), default="automatic")
    stage_parser.add_argument("--exact-version")
    stage_parser.add_argument("--github-output")
    verify_parser = sub.add_parser("verify-commit")
    verify_parser.add_argument("--commit")
    verify_parser.add_argument("--source-sha")
    verify_parser.add_argument("--version")
    verify_parser.add_argument("--github-output")
    promote_parser = sub.add_parser("promote")
    promote_parser.add_argument("--commit", required=True)
    promote_parser.add_argument("--exact-version")
    promote_parser.add_argument("--github-output")
    args = parser.parse_args(argv)
    try:
        if args.command == "stage":
            stage(args)
        elif args.command == "verify-commit":
            check_commit(args)
        else:
            promote(args)
        return 0
    except (ReleaseChainError, PRODUCT_VERSION.VersionError) as error:
        print(f"release_chain.py: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
