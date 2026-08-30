#!/usr/bin/env python3
"""Prepare the immutable VERSION commit on an already-open product PR.

This controller is executed from the trusted default-branch checkout.  It
receives PR metadata as JSON, validates the completed checks, then writes only
to the PR head worktree.  It never writes ``main``.
"""

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


class PreparationError(RuntimeError):
    pass


def git(repo_root: Path, *args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args], cwd=repo_root, text=True, capture_output=True
    )
    if check and result.returncode:
        raise PreparationError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result.stdout.strip()


def read_json(path: Path) -> object:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise PreparationError(f"cannot read JSON at {path}: {error}") from error


def latest_check_outcomes(payload: object) -> dict[str, str]:
    rows = payload.get("check_runs") if isinstance(payload, dict) else None
    if not isinstance(rows, list):
        raise PreparationError("check-runs JSON is missing check_runs")
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


def incomplete_source_checks(payload: object) -> list[str]:
    outcomes = latest_check_outcomes(payload)
    return sorted(name for name in REQUIRED_SOURCE_CHECKS if outcomes.get(name) != "success")


def require_completed_checks(payload: object) -> None:
    failed = incomplete_source_checks(payload)
    if failed:
        raise PreparationError(f"source checks are not all successful: {failed}")


def source_is_ready(repo_root: Path, source_sha: str, base_sha: str) -> None:
    source = git(repo_root, "rev-parse", f"{source_sha}^{{commit}}")
    base = git(repo_root, "rev-parse", f"{base_sha}^{{commit}}")
    if source != source_sha:
        raise PreparationError(f"checked out source is {source}, expected {source_sha}")
    if git(repo_root, "merge-base", base, source) != base:
        raise PreparationError("PR source is not based on the current main head")
    changed = git(repo_root, "diff", "--name-only", f"{base}...{source}").splitlines()
    if "VERSION" in changed:
        raise PreparationError("source commits must not modify VERSION before release preparation")


def read_intent(labels_path: Path) -> tuple[str, str, str, str]:
    labels = read_json(labels_path)
    if not isinstance(labels, list):
        raise PreparationError("PR labels JSON must be an array")
    type_label, channel_label, components = SNAPSHOT.validate_intent_labels(labels, "on prepared PR")
    channel = channel_label.split(":", 1)[1]
    components_value = ",".join(components) if components else "none"
    if type_label in {"type:docs", "type:skip"}:
        return type_label, channel, components_value, "skip"
    return type_label, channel, components_value, CHAIN.release_action(type_label, channel)


def write_outputs(values: dict[str, str], output_path: str | None) -> None:
    if output_path:
        with Path(output_path).open("a", encoding="utf-8") as output:
            for key, value in values.items():
                output.write(f"{key}={value}\n")


def prepare(args: argparse.Namespace) -> None:
    repo_root = args.repo_root.resolve()
    if not repo_root.is_dir():
        raise PreparationError(f"repository worktree does not exist: {repo_root}")
    type_label, channel, components, action = read_intent(args.labels_json)
    incomplete_checks = incomplete_source_checks(read_json(args.checks_json))
    if incomplete_checks:
        values = {
            "release_enabled": "false",
            "release_action": action,
            "release_reason": "source_checks_not_ready",
            "source_sha": args.source_sha,
            "prepared": "waiting",
        }
        write_outputs(values, args.github_output)
        print(json.dumps(values, sort_keys=True))
        return

    CHAIN.ROOT = repo_root
    try:
        existing = CHAIN.verify_prepared_commit(args.source_sha)
    except CHAIN.ReleaseChainError:
        existing = None
    if existing is not None:
        source_is_ready(repo_root, existing["sourceSha"], args.base_sha)
        if (
            existing["typeLabel"] != type_label
            or existing["channel"] != channel
            or existing["components"] != components
        ):
            raise PreparationError("prepared VERSION commit does not match the current validated labels")
        values = {
            "release_enabled": "true",
            "release_action": existing["action"],
            "release_sha": existing["releaseSha"],
            "source_sha": existing["sourceSha"],
            "version": existing["version"],
            "tag": existing["tag"],
            "prepared": "existing",
        }
        write_outputs(values, args.github_output)
        print(json.dumps(values, sort_keys=True))
        return

    source_is_ready(repo_root, args.source_sha, args.base_sha)

    if action == "skip":
        values = {
            "release_enabled": "false",
            "release_action": "skip",
            "release_reason": "skip_type_label",
            "source_sha": args.source_sha,
            "prepared": "not_required",
        }
        write_outputs(values, args.github_output)
        print(json.dumps(values, sort_keys=True))
        return

    if args.mode == "automatic" and action == "exact":
        values = {
            "release_enabled": "false",
            "release_action": "exact",
            "release_reason": "controlled_exact_required",
            "source_sha": args.source_sha,
            "prepared": "waiting_for_exact",
        }
        write_outputs(values, args.github_output)
        print(json.dumps(values, sort_keys=True))
        return
    if args.mode != action:
        raise PreparationError(f"validated labels require {action} preparation, got {args.mode}")
    if args.mode == "exact" and not args.exact_version:
        raise PreparationError("exact preparation requires --exact-version")

    CHAIN.stage(
        argparse.Namespace(
            source_sha=args.source_sha,
            mode=args.mode,
            exact_version=args.exact_version,
            expected_channel=channel,
            intent_type=type_label,
            intent_channel=channel,
            intent_components=components,
            github_output=None,
        )
    )
    prepared = CHAIN.verify_prepared_commit(git(repo_root, "rev-parse", "HEAD"), args.source_sha)
    values = {
        "release_enabled": "true",
        "release_action": prepared["action"],
        "release_sha": prepared["releaseSha"],
        "source_sha": prepared["sourceSha"],
        "version": prepared["version"],
        "tag": prepared["tag"],
        "prepared": "created",
    }
    write_outputs(values, args.github_output)
    print(json.dumps(values, sort_keys=True))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--base-sha", required=True)
    parser.add_argument("--labels-json", type=Path, required=True)
    parser.add_argument("--checks-json", type=Path, required=True)
    parser.add_argument("--mode", choices=("automatic", "exact"), required=True)
    parser.add_argument("--exact-version")
    parser.add_argument("--github-output")
    args = parser.parse_args(argv)
    try:
        prepare(args)
        return 0
    except (PreparationError, CHAIN.ReleaseChainError, SNAPSHOT.SnapshotError, CHAIN.PRODUCT_VERSION.VersionError) as error:
        print(f"release_preparation.py: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
