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


VALID_TYPES = {"type:patch", "type:minor", "type:major"}
VALID_CHANNELS = {"stable", "rc"}


def release_action(type_label: str, channel: str) -> str:
    if type_label not in VALID_TYPES:
        raise ReleaseChainError(f"unsupported release intent type: {type_label}")
    if channel not in VALID_CHANNELS:
        raise ReleaseChainError(f"unsupported release intent channel: {channel}")
    return "automatic" if type_label == "type:patch" and channel == "stable" else "exact"


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
        raise ReleaseChainError(f"VERSION preparation commit {commit} must have exactly one parent")
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


def prepared_intent(commit: str) -> dict[str, str]:
    """Return the immutable PR-label intent copied into a preparation commit."""
    values = trailers(commit)
    type_label = values.get("Release-Intent-Type", "")
    channel = values.get("Release-Intent-Channel", "")
    action = values.get("Release-Intent-Action", "")
    if type_label not in VALID_TYPES:
        raise ReleaseChainError("VERSION preparation commit is missing a valid Release-Intent-Type trailer")
    if channel not in VALID_CHANNELS:
        raise ReleaseChainError("VERSION preparation commit is missing a valid Release-Intent-Channel trailer")
    if action != release_action(type_label, channel):
        raise ReleaseChainError("VERSION preparation commit has an invalid Release-Intent-Action trailer")
    components = values.get("Release-Intent-Components", "none")
    return {
        "typeLabel": type_label,
        "channel": channel,
        "action": action,
        "components": components,
    }


def is_strictly_newer(candidate: str, current: str) -> bool:
    """Compare the supported stable/RC forms without consulting release state."""
    candidate_parts = PRODUCT_VERSION.parse_version(candidate)
    current_parts = PRODUCT_VERSION.parse_version(current)
    candidate_core = tuple(int(candidate_parts[key]) for key in ("major", "minor", "patch"))
    current_core = tuple(int(current_parts[key]) for key in ("major", "minor", "patch"))
    if candidate_core != current_core:
        return candidate_core > current_core
    candidate_rc = candidate_parts["rc"]
    current_rc = current_parts["rc"]
    if candidate_rc is None:
        return current_rc is not None
    return current_rc is not None and int(candidate_rc) > int(current_rc)


def verify_release_commit(commit: str, source_sha: str | None = None, expected_version: str | None = None) -> dict[str, str]:
    commit = git("rev-parse", f"{commit}^{{commit}}")
    parent = commit_parent(commit)
    if source_sha and parent != source_sha:
        raise ReleaseChainError(f"VERSION preparation commit {commit} parent is {parent}, expected {source_sha}")
    names = diff_names(commit)
    if names != ["VERSION"]:
        raise ReleaseChainError(f"VERSION preparation commit {commit} must change only VERSION, got {names}")
    version = commit_version(commit)
    if expected_version and version != expected_version:
        raise ReleaseChainError(f"VERSION preparation commit has VERSION {version}, expected {expected_version}")
    commit_trailers = trailers(commit)
    if commit_trailers.get("Release-Source-SHA") != parent:
        raise ReleaseChainError("VERSION preparation commit is missing a matching Release-Source-SHA trailer")
    if commit_trailers.get("Product-Version") != version:
        raise ReleaseChainError("VERSION preparation commit is missing a matching Product-Version trailer")
    return {"releaseSha": commit, "sourceSha": parent, "version": version, "tag": f"v{version}"}


def verify_prepared_commit(
    commit: str, source_sha: str | None = None, expected_version: str | None = None
) -> dict[str, str]:
    values = verify_release_commit(commit, source_sha, expected_version)
    values.update(prepared_intent(values["releaseSha"]))
    return values


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
        raise ReleaseChainError("source checkout must be clean before staging a VERSION preparation commit")
    try:
        verify_release_commit(source_sha)
    except ReleaseChainError:
        pass
    else:
        raise ReleaseChainError(f"source {source_sha} is already a VERSION preparation commit")
    current = PRODUCT_VERSION.read_version(ROOT / "VERSION")
    if args.mode == "automatic":
        version = PRODUCT_VERSION.next_patch(current)
    elif args.mode == "exact":
        if not args.exact_version:
            raise ReleaseChainError("exact mode requires --exact-version")
        parsed = PRODUCT_VERSION.parse_version(args.exact_version)
        if args.expected_channel:
            channel = "rc" if parsed["prerelease"] else "stable"
            if channel != args.expected_channel:
                raise ReleaseChainError(
                    f"exact VERSION channel is {channel}, expected {args.expected_channel} from frozen release intent"
                )
        if not is_strictly_newer(args.exact_version, current):
            raise ReleaseChainError("exact VERSION must be strictly newer than the current VERSION")
        version = args.exact_version
    else:
        raise ReleaseChainError(f"unsupported staging mode: {args.mode}")

    intent_type = getattr(args, "intent_type", None)
    intent_channel = getattr(args, "intent_channel", None)
    intent_components = getattr(args, "intent_components", None)
    if (intent_type is None) != (intent_channel is None):
        raise ReleaseChainError("Release intent type and channel must be supplied together")
    intent_action = ""
    if intent_type is not None:
        intent_action = release_action(intent_type, intent_channel)
        if args.mode != intent_action:
            raise ReleaseChainError(f"{intent_type} + {intent_channel} requires {intent_action} staging")
        version_channel = "rc" if PRODUCT_VERSION.parse_version(version)["prerelease"] else "stable"
        if version_channel != intent_channel:
            raise ReleaseChainError("VERSION channel does not match the frozen release intent")
    (ROOT / "VERSION").write_text(version + "\n", encoding="utf-8")
    if git("diff", "--name-only") != "VERSION":
        raise ReleaseChainError("staging a VERSION preparation commit may modify only VERSION")
    subprocess.run(
        [
            "git",
            "add",
            "VERSION",
        ],
        cwd=ROOT,
        check=True,
    )
    metadata = [
        f"Release-Source-SHA: {source_sha}",
        f"Product-Version: {version}",
    ]
    if intent_type is not None:
        metadata.extend(
            [
                f"Release-Intent-Type: {intent_type}",
                f"Release-Intent-Channel: {intent_channel}",
                f"Release-Intent-Action: {intent_action}",
                f"Release-Intent-Components: {intent_components or 'none'}",
            ]
        )
    command = [
        "git",
        "commit",
        "--signoff",
        "-m",
        f"chore(release): v{version}",
        "-m",
        "\n".join(metadata),
    ]
    subprocess.run(command, cwd=ROOT, check=True)
    release_sha = git("rev-parse", "HEAD")
    values = (
        verify_prepared_commit(release_sha, source_sha, version)
        if intent_type is not None
        else verify_release_commit(release_sha, source_sha, version)
    )
    write_github_output(values, args.github_output)
    print(json.dumps(values, sort_keys=True))


def promote(args: argparse.Namespace) -> None:
    release = verify_release_commit(args.commit)
    checked_out = git("rev-parse", "HEAD")
    if checked_out != release["releaseSha"]:
        raise ReleaseChainError(
            f"promotion checkout is {checked_out}, expected RC VERSION preparation commit {release['releaseSha']}"
        )
    parsed = PRODUCT_VERSION.parse_version(release["version"])
    if not parsed["prerelease"]:
        raise ReleaseChainError("promotion requires an RC VERSION preparation commit")
    stable_version = f"{parsed['major']}.{parsed['minor']}.{parsed['patch']}"
    if args.exact_version and args.exact_version != stable_version:
        raise ReleaseChainError("promotion version must remove only the RC prerelease")
    (ROOT / "VERSION").write_text(stable_version + "\n", encoding="utf-8")
    if git("diff", "--name-only") != "VERSION":
        raise ReleaseChainError("promotion VERSION preparation commit may modify only VERSION")
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


def check_prepared_commit(args: argparse.Namespace) -> None:
    commit = args.commit or git("rev-parse", "HEAD")
    values = verify_prepared_commit(commit, args.source_sha, args.version)
    write_github_output(values, args.github_output)
    print(json.dumps(values, sort_keys=True))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    stage_parser = sub.add_parser("stage")
    stage_parser.add_argument("--source-sha", required=True)
    stage_parser.add_argument("--mode", choices=("automatic", "exact"), default="automatic")
    stage_parser.add_argument("--exact-version")
    stage_parser.add_argument("--expected-channel", choices=("stable", "rc"))
    stage_parser.add_argument("--intent-type", choices=tuple(sorted(VALID_TYPES)))
    stage_parser.add_argument("--intent-channel", choices=tuple(sorted(VALID_CHANNELS)))
    stage_parser.add_argument("--intent-components", default="none")
    stage_parser.add_argument("--github-output")
    verify_parser = sub.add_parser("verify-commit")
    verify_parser.add_argument("--commit")
    verify_parser.add_argument("--source-sha")
    verify_parser.add_argument("--version")
    verify_parser.add_argument("--github-output")
    verify_prepared_parser = sub.add_parser("verify-prepared")
    verify_prepared_parser.add_argument("--commit")
    verify_prepared_parser.add_argument("--source-sha")
    verify_prepared_parser.add_argument("--version")
    verify_prepared_parser.add_argument("--github-output")
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
        elif args.command == "verify-prepared":
            check_prepared_commit(args)
        else:
            promote(args)
        return 0
    except (ReleaseChainError, PRODUCT_VERSION.VersionError) as error:
        print(f"release_chain.py: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
