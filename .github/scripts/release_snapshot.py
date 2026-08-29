#!/usr/bin/env python3
"""Freeze and reconcile PR release intent on the mainline release train.

The snapshot records the PR labels that selected the release operation.  It is
deliberately not a product-version source: numeric versions are resolved by
``release_chain.py`` from the checked-out root VERSION file.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any
from urllib import error, request

ROOT = Path(__file__).resolve().parents[2]
SCHEMA_VERSION = 2
DEFAULT_NOTES_REF = "refs/notes/release-snapshots"
VALID_TYPES = {"type:patch", "type:minor", "type:major", "type:docs", "type:skip"}
VALID_CHANNELS = {"channel:stable", "channel:rc"}
VALID_COMPONENTS = {"component:web", "component:firmware", "component:host-tools", "component:docs"}
INTENT_MARKER = "<!-- flux-purr-release-intent:v2 -->"
LEGACY_INTENT_MARKER = "<!-- flux-purr-release-intent:v1 -->"
SHA_RE = re.compile(r"^[0-9a-f]{40}$")


class SnapshotError(RuntimeError):
    pass


def git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(["git", *args], cwd=ROOT, text=True, capture_output=True)
    if check and result.returncode:
        raise SnapshotError(result.stderr.strip() or f"git {' '.join(args)} failed")
    return result


def git_output(*args: str) -> str:
    return git(*args).stdout.strip()


def github_request(api_root: str, token: str, repository: str, path: str, method: str = "GET", payload: Any = None) -> Any:
    body = json.dumps(payload).encode("utf-8") if payload is not None else None
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "flux-purr-release-snapshot",
    }
    if body is not None:
        headers["Content-Type"] = "application/json"
    req = request.Request(f"{api_root.rstrip('/')}/repos/{repository}{path}", data=body, headers=headers, method=method)
    try:
        with request.urlopen(req) as response:
            raw = response.read().decode("utf-8")
            return json.loads(raw) if raw else None
    except error.HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        raise SnapshotError(f"GitHub API error on {path}: {exc.code} {detail}") from exc


def github_json(api_root: str, token: str, repository: str, path: str) -> Any:
    return github_request(api_root, token, repository, path)


def labels_with_prefix(labels: list[dict[str, Any]], prefix: str) -> list[str]:
    return sorted(label["name"] for label in labels if isinstance(label, dict) and isinstance(label.get("name"), str) and label["name"].startswith(prefix))


def validate_intent_labels(labels: list[dict[str, Any]], context: str = "") -> tuple[str, str, list[str]]:
    type_labels = labels_with_prefix(labels, "type:")
    channel_labels = labels_with_prefix(labels, "channel:")
    component_labels = labels_with_prefix(labels, "component:")
    if len(type_labels) != 1:
        raise SnapshotError(f"Expected exactly one type:* label {context}, got {type_labels or ['none']}")
    if len(channel_labels) != 1:
        raise SnapshotError(f"Expected exactly one channel:* label {context}, got {channel_labels or ['none']}")
    if type_labels[0] not in VALID_TYPES:
        raise SnapshotError(f"Unsupported type label {context}: {type_labels[0]}")
    if channel_labels[0] not in VALID_CHANNELS:
        raise SnapshotError(f"Unsupported channel label {context}: {channel_labels[0]}")
    unknown_components = sorted(set(component_labels) - VALID_COMPONENTS)
    if unknown_components:
        raise SnapshotError(f"Unsupported component labels {context}: {unknown_components}")
    return type_labels[0], channel_labels[0], component_labels


def release_fields(type_label: str, channel_label: str) -> tuple[bool, str, str]:
    if type_label in {"type:docs", "type:skip"}:
        return False, "", "skip_type_label"
    return True, type_label.split(":", 1)[1], "pr_labels"


def release_action(payload: dict[str, Any]) -> str:
    """Map frozen label intent to a controller operation, never a version number."""
    if payload["release_enabled"] is False:
        return "skip"
    if payload["type_label"] == "type:patch" and payload["channel_label"] == "channel:stable":
        return "automatic"
    return "exact"


def validate_snapshot(payload: Any, target_sha: str) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise SnapshotError("unsupported release snapshot schema")
    if payload.get("schema_version") == 1:
        if payload.get("target_sha") != target_sha:
            raise SnapshotError(f"release snapshot target mismatch for {target_sha}")
        type_label = payload.get("type_label")
        channel_label = payload.get("channel_label")
        if type_label not in VALID_TYPES or channel_label not in VALID_CHANNELS:
            raise SnapshotError("legacy release snapshot has invalid release labels")
        enabled, level, _ = release_fields(type_label, channel_label)
        legacy = dict(payload)
        legacy.setdefault("merge_commit_sha", target_sha)
        legacy.setdefault("release_enabled", enabled)
        legacy.setdefault("release_level", level)
        legacy.setdefault("release_channel", channel_label.split(":", 1)[1])
        legacy.setdefault("components", [])
        legacy.setdefault("labels", [type_label, channel_label, *legacy["components"]])
        legacy.setdefault("version_source", "historical snapshot; VERSION is authoritative")
        return legacy
    if payload.get("schema_version") != SCHEMA_VERSION:
        raise SnapshotError("unsupported release snapshot schema")
    if payload.get("target_sha") != target_sha or not SHA_RE.fullmatch(target_sha):
        raise SnapshotError(f"release snapshot target mismatch for {target_sha}")
    if payload.get("merge_commit_sha") != target_sha:
        raise SnapshotError("release snapshot merge commit does not match target")
    type_label = payload.get("type_label")
    channel_label = payload.get("channel_label")
    if type_label not in VALID_TYPES or channel_label not in VALID_CHANNELS:
        raise SnapshotError("release snapshot has invalid release labels")
    enabled, level, _ = release_fields(type_label, channel_label)
    if payload.get("release_enabled") is not enabled or payload.get("release_level", "") != level:
        raise SnapshotError("release snapshot release fields do not match labels")
    if payload.get("release_channel") != channel_label.split(":", 1)[1]:
        raise SnapshotError("release snapshot channel does not match channel label")
    labels = payload.get("labels")
    if not isinstance(labels, list) or type_label not in labels or channel_label not in labels:
        raise SnapshotError("release snapshot labels are incomplete")
    if not isinstance(payload.get("pr_number"), int) or not SHA_RE.fullmatch(str(payload.get("pr_head_sha", ""))):
        raise SnapshotError("release snapshot is missing PR identity")
    return payload


def read_snapshot(notes_ref: str, target_sha: str) -> dict[str, Any] | None:
    result = git("notes", f"--ref={notes_ref}", "show", target_sha, check=False)
    if result.returncode:
        return None
    try:
        return validate_snapshot(json.loads(result.stdout), target_sha)
    except json.JSONDecodeError as exc:
        raise SnapshotError(f"snapshot for {target_sha} is not valid JSON") from exc


def fetch_notes(notes_ref: str) -> None:
    remote = git("ls-remote", "--exit-code", "origin", notes_ref, check=False)
    if remote.returncode == 0:
        git("fetch", "--no-tags", "origin", f"+{notes_ref}:{notes_ref}")


def intent_comment_body(payload: dict[str, Any]) -> str:
    return f"{INTENT_MARKER}\n```json\n{json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=2)}\n```"


def parse_intent_comment(body: str) -> dict[str, Any] | None:
    marker = INTENT_MARKER if INTENT_MARKER in body else LEGACY_INTENT_MARKER if LEGACY_INTENT_MARKER in body else None
    if marker is None:
        return None
    value = body.split(marker, 1)[1].strip().removeprefix("```json").strip()
    if value.endswith("```"):
        value = value[:-3].strip()
    payload = json.loads(value)
    if not isinstance(payload, dict):
        raise SnapshotError("release intent marker must contain a JSON object")
    return payload


def pr_comments(api_root: str, token: str, repository: str, pr_number: int) -> list[dict[str, Any]]:
    page = 1
    result: list[dict[str, Any]] = []
    while True:
        page_items = github_json(api_root, token, repository, f"/issues/{pr_number}/comments?per_page=100&page={page}")
        if not isinstance(page_items, list):
            raise SnapshotError("PR comments response must be a list")
        result.extend(item for item in page_items if isinstance(item, dict))
        if len(page_items) < 100:
            return result
        page += 1


def trusted_comment(comment: dict[str, Any]) -> bool:
    user = comment.get("user")
    return isinstance(user, dict) and user.get("login") == "github-actions[bot]" and user.get("type") == "Bot"


def load_frozen_intent(api_root: str, token: str, repository: str, pr_number: int, head_sha: str) -> dict[str, Any]:
    matches = []
    for comment in pr_comments(api_root, token, repository, pr_number):
        if not trusted_comment(comment) or not isinstance(comment.get("body"), str):
            continue
        payload = parse_intent_comment(comment["body"])
        if payload and payload.get("pr_number") == pr_number and payload.get("pr_head_sha") == head_sha:
            matches.append(payload)
    if not matches:
        raise SnapshotError(f"no frozen release intent marker for PR #{pr_number} head {head_sha}")
    type_label = matches[-1].get("type_label")
    channel_label = matches[-1].get("channel_label")
    if type_label not in VALID_TYPES or channel_label not in VALID_CHANNELS:
        raise SnapshotError("frozen release intent contains invalid labels")
    return {"type_label": type_label, "channel_label": channel_label, "components": matches[-1].get("components", [])}


def write_frozen_intent(api_root: str, token: str, repository: str, payload: dict[str, Any]) -> None:
    body = intent_comment_body(payload)
    existing_id: int | None = None
    for comment in pr_comments(api_root, token, repository, payload["pr_number"]):
        if not trusted_comment(comment) or not isinstance(comment.get("body"), str):
            continue
        marker = parse_intent_comment(comment["body"])
        if marker and marker.get("pr_head_sha") == payload["pr_head_sha"]:
            existing_id = comment.get("id") if isinstance(comment.get("id"), int) else None
    if existing_id is None:
        github_request(api_root, token, repository, f"/issues/{payload['pr_number']}/comments", "POST", {"body": body})
    else:
        github_request(api_root, token, repository, f"/issues/comments/{existing_id}", "PATCH", {"body": body})


def capture_intent(args: argparse.Namespace) -> None:
    event = json.loads(Path(args.event_path).read_text(encoding="utf-8"))
    pr = event.get("pull_request")
    if not isinstance(pr, dict) or pr.get("state") != "open":
        return
    number = pr.get("number")
    head = pr.get("head") if isinstance(pr.get("head"), dict) else {}
    head_sha = head.get("sha")
    labels = pr.get("labels")
    if not isinstance(number, int) or not isinstance(head_sha, str) or not SHA_RE.fullmatch(head_sha) or not isinstance(labels, list):
        raise SnapshotError("pull request event is missing release intent fields")
    type_label, channel_label, components = validate_intent_labels(labels, f"on PR #{number}")
    enabled, level, reason = release_fields(type_label, channel_label)
    write_frozen_intent(args.api_root, args.github_token, args.github_repository, {
        "schema_version": SCHEMA_VERSION,
        "pr_number": number,
        "pr_title": pr.get("title", "") if isinstance(pr.get("title"), str) else "",
        "pr_head_sha": head_sha,
        "labels": [type_label, channel_label, *components],
        "type_label": type_label,
        "channel_label": channel_label,
        "components": components,
        "release_enabled": enabled,
        "release_level": level,
        "release_channel": channel_label.split(":", 1)[1],
        "release_reason": reason,
    })
    print(f"Frozen release intent recorded for PR #{number}: {type_label} + {channel_label} @ {head_sha[:7]}")


def build_snapshot(api_root: str, token: str, repository: str, target_sha: str) -> dict[str, Any]:
    prs = github_json(api_root, token, repository, f"/commits/{target_sha}/pulls")
    if not isinstance(prs, list) or len(prs) != 1:
        raise SnapshotError(f"expected exactly one PR associated with {target_sha}")
    pr = prs[0]
    number = pr.get("number")
    head = pr.get("head") if isinstance(pr.get("head"), dict) else {}
    head_sha = head.get("sha")
    if not isinstance(number, int) or not isinstance(head_sha, str) or not SHA_RE.fullmatch(head_sha):
        raise SnapshotError("associated PR is missing identity")
    intent = load_frozen_intent(api_root, token, repository, number, head_sha)
    type_label, channel_label, components = validate_intent_labels(
        [{"name": intent["type_label"]}, {"name": intent["channel_label"]}, *({"name": c} for c in intent.get("components", []))],
        f"for PR #{number}",
    )
    enabled, level, reason = release_fields(type_label, channel_label)
    return {
        "schema_version": SCHEMA_VERSION,
        "target_sha": target_sha,
        "snapshot_source": "frozen_pr_marker",
        "pr_number": number,
        "pr_title": pr.get("title", "") if isinstance(pr.get("title"), str) else "",
        "pr_head_sha": head_sha,
        "merge_commit_sha": target_sha,
        "labels": [type_label, channel_label, *components],
        "type_label": type_label,
        "channel_label": channel_label,
        "components": components,
        "release_enabled": enabled,
        "release_level": level,
        "release_channel": channel_label.split(":", 1)[1],
        "release_reason": f"frozen_{reason}",
        "version_source": "VERSION",
    }


def add_note(notes_ref: str, target_sha: str, payload: dict[str, Any]) -> None:
    git("notes", f"--ref={notes_ref}", "add", "-f", "-m", json.dumps(payload, sort_keys=True), target_sha)


def ensure(args: argparse.Namespace) -> None:
    if not SHA_RE.fullmatch(args.target_sha):
        raise SnapshotError("target SHA must be a full lowercase commit SHA")
    git("cat-file", "-e", f"{args.target_sha}^{{commit}}")
    fetch_notes(args.notes_ref)
    existing = read_snapshot(args.notes_ref, args.target_sha)
    if existing is None:
        payload = build_snapshot(args.api_root, args.github_token, args.github_repository, args.target_sha)
        add_note(args.notes_ref, args.target_sha, payload)
        for attempt in range(3):
            pushed = git("push", "origin", f"{args.notes_ref}:{args.notes_ref}", check=False)
            if pushed.returncode == 0:
                break
            if attempt == 2:
                raise SnapshotError(pushed.stderr.strip() or "release snapshot notes push failed")
            fetch_notes(args.notes_ref)
            if read_snapshot(args.notes_ref, args.target_sha) is None:
                add_note(args.notes_ref, args.target_sha, payload)
        existing = read_snapshot(args.notes_ref, args.target_sha)
    if existing is None:
        raise SnapshotError(f"failed to materialize release snapshot for {args.target_sha}")
    output = Path(args.output)
    output.write_text(json.dumps(existing, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(f"Release snapshot ready for {args.target_sha}: {existing['type_label']} + {existing['channel_label']}")


def write_outputs(payload: dict[str, Any], output_path: str) -> None:
    values = {
        "release_enabled": str(payload["release_enabled"]).lower(),
        "release_action": release_action(payload),
        "release_level": payload.get("release_level", ""),
        "release_channel": payload["release_channel"],
        "release_reason": payload["release_reason"],
        "type_label": payload["type_label"],
        "channel_label": payload["channel_label"],
        "pr_number": str(payload["pr_number"]),
        "components": ",".join(payload.get("components", [])),
    }
    if output_path:
        with Path(output_path).open("a", encoding="utf-8") as stream:
            for key, value in values.items():
                stream.write(f"{key}={value}\n")
    else:
        print("".join(f"{key}={value}\n" for key, value in values.items()), end="")


def resolve(args: argparse.Namespace) -> None:
    fetch_notes(args.notes_ref)
    payload = read_snapshot(args.notes_ref, args.target_sha)
    if payload is None:
        raise SnapshotError(f"no release snapshot found for {args.target_sha}")
    if args.operation == "promote" and payload["release_channel"] != "rc":
        raise SnapshotError("promotion requires an RC release snapshot")
    write_outputs(payload, args.github_output)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    capture = sub.add_parser("capture-intent")
    capture.add_argument("--event-path", default=os.environ.get("GITHUB_EVENT_PATH", ""))
    capture.add_argument("--github-repository", required=True)
    capture.add_argument("--github-token", required=True)
    capture.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    ensure_parser = sub.add_parser("ensure")
    ensure_parser.add_argument("--target-sha", required=True)
    ensure_parser.add_argument("--github-repository", required=True)
    ensure_parser.add_argument("--github-token", required=True)
    ensure_parser.add_argument("--notes-ref", default=DEFAULT_NOTES_REF)
    ensure_parser.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    ensure_parser.add_argument("--output", required=True)
    resolve_parser = sub.add_parser("resolve")
    resolve_parser.add_argument("--operation", choices=("automatic", "exact", "recover", "promote"), required=True)
    resolve_parser.add_argument("--target-sha", required=True)
    resolve_parser.add_argument("--notes-ref", default=DEFAULT_NOTES_REF)
    resolve_parser.add_argument("--github-output", default=os.environ.get("GITHUB_OUTPUT", ""))
    args = parser.parse_args(argv)
    try:
        if args.command == "capture-intent":
            capture_intent(args)
        elif args.command == "ensure":
            ensure(args)
        else:
            resolve(args)
        return 0
    except (SnapshotError, json.JSONDecodeError) as error:
        print(f"release_snapshot.py: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
