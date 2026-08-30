#!/usr/bin/env python3
"""Enforce the PR-local VERSION preparation contract before a main merge."""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / ".github/scripts"))
import release_chain as CHAIN  # noqa: E402
import release_snapshot as SNAPSHOT  # noqa: E402


REQUIRED_SOURCE_CHECKS = {
    "Validate PR labels",
    "Firmware checks",
    "DEVD checks",
    "Web checks",
    "Worktree bootstrap",
}


class CompletionError(RuntimeError):
    pass


def git(*args: str, check: bool = True) -> str:
    result = subprocess.run(["git", *args], cwd=ROOT, text=True, capture_output=True)
    if check and result.returncode:
        raise CompletionError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result.stdout.strip()


def has_version(commit: str) -> bool:
    return subprocess.run(
        ["git", "cat-file", "-e", f"{commit}:VERSION"],
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    ).returncode == 0


def show_version(commit: str) -> str:
    try:
        value = subprocess.check_output(["git", "show", f"{commit}:VERSION"], cwd=ROOT, text=True)
    except subprocess.CalledProcessError as error:
        raise CompletionError(f"{commit} has no VERSION file") from error
    return CHAIN.PRODUCT_VERSION.read_version_from_text(value)


def read_labels(path: Path | None) -> tuple[str, str, str]:
    if path is None:
        raise CompletionError("--labels-json is required for a VERSION-enabled base")
    try:
        labels = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CompletionError(f"cannot read PR labels: {error}") from error
    if not isinstance(labels, list):
        raise CompletionError("PR labels JSON must be an array")
    type_label, channel_label, components = SNAPSHOT.validate_intent_labels(labels, "on release completion")
    return type_label, channel_label.split(":", 1)[1], ",".join(components) if components else "none"


def latest_check_outcomes(payload: object) -> dict[str, str]:
    rows = payload.get("check_runs") if isinstance(payload, dict) else None
    if not isinstance(rows, list):
        raise CompletionError("source check results are missing check_runs")
    outcomes: dict[str, tuple[str, str]] = {}
    for index, row in enumerate(rows):
        if not isinstance(row, dict):
            continue
        name = row.get("name")
        conclusion = row.get("conclusion")
        if isinstance(name, str) and isinstance(conclusion, str):
            completed_at = row.get("completed_at")
            started_at = row.get("started_at")
            timestamp = completed_at if isinstance(completed_at, str) else started_at if isinstance(started_at, str) else ""
            candidate = (f"{timestamp}:{index:06d}", conclusion)
            if name not in outcomes or candidate[0] >= outcomes[name][0]:
                outcomes[name] = candidate
    return {name: value for name, (_, value) in outcomes.items()}


def require_completed_source_checks(path: Path | None) -> None:
    if path is None:
        raise CompletionError("--checks-json is required for a product VERSION preparation")
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise CompletionError(f"cannot read source check results: {error}") from error
    outcomes = latest_check_outcomes(payload)
    failed = sorted(name for name in REQUIRED_SOURCE_CHECKS if outcomes.get(name) != "success")
    if failed:
        raise CompletionError(f"prepared source checks are not all successful: {failed}")


def verify_source(base: str, source: str) -> None:
    if git("merge-base", base, source) != base:
        raise CompletionError("prepared source is not based on the current main head")
    names = git("diff", "--name-only", f"{base}...{source}").splitlines()
    if "VERSION" in names:
        raise CompletionError("source commits must not modify VERSION before release preparation")


def verify_prepared(base: str, commit: str, type_label: str, channel: str, components: str) -> dict[str, str]:
    release = CHAIN.verify_prepared_commit(commit)
    verify_source(base, release["sourceSha"])
    if release["typeLabel"] != type_label or release["channel"] != channel:
        raise CompletionError("prepared VERSION commit does not match the currently validated labels")
    if release["components"] != components:
        raise CompletionError("prepared VERSION commit does not match the currently validated component labels")
    expected_action = CHAIN.release_action(type_label, channel)
    if release["action"] != expected_action:
        raise CompletionError("prepared VERSION commit has an invalid release action")
    base_version = show_version(base)
    if expected_action == "automatic":
        expected_version = CHAIN.PRODUCT_VERSION.next_patch(base_version)
        if release["version"] != expected_version:
            raise CompletionError(
                f"automatic preparation must write {expected_version}, got {release['version']}"
            )
    else:
        if not CHAIN.is_strictly_newer(release["version"], base_version):
            raise CompletionError("exact VERSION must be strictly newer than main VERSION")
        version_channel = "rc" if CHAIN.PRODUCT_VERSION.parse_version(release["version"])["prerelease"] else "stable"
        if version_channel != channel:
            raise CompletionError("exact VERSION channel does not match the validated labels")
    return release


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--commit", default="HEAD", help="pull request head commit")
    parser.add_argument("--base", default="origin/main", help="pull request base commit")
    parser.add_argument("--labels-json", type=Path)
    parser.add_argument("--checks-json", type=Path)
    parser.add_argument("--allow-migration", action="store_true")
    parser.add_argument("--migration-version", default="0.22.0")
    args = parser.parse_args(argv)
    try:
        commit = git("rev-parse", f"{args.commit}^{{commit}}")
        base = git("rev-parse", f"{args.base}^{{commit}}")
        if not has_version(base):
            if not args.allow_migration:
                raise CompletionError("base branch has no VERSION; migration permission is required")
            if show_version(commit) != args.migration_version:
                raise CompletionError(f"migration must establish VERSION={args.migration_version}")
            print(f"release completion: migration baseline {args.migration_version} allowed")
            return 0

        type_label, channel, components = read_labels(args.labels_json)
        changed = git("diff", "--name-only", f"{base}...{commit}").splitlines()
        if "VERSION" not in changed:
            if type_label in {"type:docs", "type:skip"}:
                print(f"release completion: {type_label} does not require product VERSION preparation")
                return 0
            raise CompletionError("product PR must end with a prepared VERSION-only commit")
        if type_label in {"type:docs", "type:skip"}:
            raise CompletionError("non-product PRs must not modify VERSION")
        release = verify_prepared(base, commit, type_label, channel, components)
        require_completed_source_checks(args.checks_json)
        print(
            f"release completion: prepared {release['tag']} at {release['releaseSha']} "
            f"for source {release['sourceSha']}"
        )
        return 0
    except (CompletionError, CHAIN.ReleaseChainError, SNAPSHOT.SnapshotError, CHAIN.PRODUCT_VERSION.VersionError) as error:
        print(f"release_completion.py: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
