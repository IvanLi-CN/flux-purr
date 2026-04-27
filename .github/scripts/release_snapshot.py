#!/usr/bin/env python3
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

SCHEMA_VERSION = 1
DEFAULT_NOTES_REF = "refs/notes/release-snapshots"
VALID_TYPES = {"type:patch", "type:minor", "type:major", "type:docs", "type:skip"}
VALID_CHANNELS = {"channel:stable", "channel:rc"}
STABLE_TAG_RE = re.compile(r"^(.+)/v([0-9]+)\.([0-9]+)\.([0-9]+)$")
INTENT_MARKER = "<!-- flux-purr-release-intent:v1 -->"


class SnapshotError(RuntimeError):
    pass


def run_git(*args: str, check: bool = True) -> subprocess.CompletedProcess[str]:
    result = subprocess.run(["git", *args], text=True, capture_output=True, check=False)
    if check and result.returncode != 0:
        detail = result.stderr.strip() or result.stdout.strip() or f"git {' '.join(args)} failed"
        raise SnapshotError(detail)
    return result


def git_output(*args: str) -> str:
    return run_git(*args).stdout.strip()


def fetch_notes(notes_ref: str) -> None:
    probe = run_git("ls-remote", "--exit-code", "origin", notes_ref, check=False)
    if probe.returncode == 0:
        run_git("fetch", "--no-tags", "origin", f"+{notes_ref}:{notes_ref}")


def stable_tag_points_at(target_sha: str) -> bool:
    for tag in git_output("tag", "--points-at", target_sha).splitlines():
        if STABLE_TAG_RE.fullmatch(tag):
            return True
    return False


def github_request(api_root: str, token: str, repository: str, path: str, method: str = "GET", payload: Any = None) -> Any:
    url = f"{api_root.rstrip('/')}/repos/{repository}{path}"
    body = None
    headers = {
        "Authorization": f"Bearer {token}",
        "Accept": "application/vnd.github+json, application/vnd.github.groot-preview+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "flux-purr-release-snapshot",
    }
    if payload is not None:
        body = json.dumps(payload).encode("utf-8")
        headers["Content-Type"] = "application/json"
    req = request.Request(
        url,
        data=body,
        headers=headers,
        method=method,
    )
    try:
        with request.urlopen(req) as resp:
            raw = resp.read().decode("utf-8")
            return json.loads(raw) if raw else None
    except error.HTTPError as exc:
        body = exc.read().decode("utf-8", errors="replace")
        raise SnapshotError(f"GitHub API error on {path}: {exc.code} {body}") from exc


def github_json(api_root: str, token: str, repository: str, path: str) -> Any:
    return github_request(api_root, token, repository, path)


def read_snapshot(notes_ref: str, target_sha: str) -> dict[str, Any] | None:
    result = run_git("notes", f"--ref={notes_ref}", "show", target_sha, check=False)
    if result.returncode != 0:
        return None
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise SnapshotError(f"Snapshot for {target_sha} is not valid JSON") from exc
    return validate_snapshot(payload, target_sha)


def validate_snapshot(payload: Any, target_sha: str) -> dict[str, Any]:
    if not isinstance(payload, dict):
        raise SnapshotError("Snapshot must be a JSON object")
    if payload.get("schema_version") != SCHEMA_VERSION:
        raise SnapshotError(f"Unsupported snapshot schema: {payload.get('schema_version')!r}")
    if payload.get("target_sha") != target_sha:
        raise SnapshotError(f"Snapshot target mismatch for {target_sha}")
    for key in ("type_label", "channel_label", "release_channel", "release_reason"):
        if not isinstance(payload.get(key), str):
            raise SnapshotError(f"Snapshot {key} must be a string")
    if payload["type_label"] not in VALID_TYPES:
        raise SnapshotError(f"Invalid snapshot type_label: {payload['type_label']}")
    if payload["channel_label"] not in VALID_CHANNELS:
        raise SnapshotError(f"Invalid snapshot channel_label: {payload['channel_label']}")
    if not isinstance(payload.get("release_enabled"), bool):
        raise SnapshotError("Snapshot release_enabled must be boolean")
    if payload["release_enabled"] and payload.get("release_level") not in {"patch", "minor", "major"}:
        raise SnapshotError("Release snapshot must include patch/minor/major release_level")
    if payload["release_enabled"]:
        components = payload.get("components")
        if not isinstance(components, dict):
            raise SnapshotError("Release snapshot must include components when release_enabled=true")
        for component in ("web", "firmware"):
            detail = components.get(component)
            if not isinstance(detail, dict):
                raise SnapshotError(f"Release snapshot missing component {component}")
            if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", str(detail.get("effective_version", ""))):
                raise SnapshotError(f"Release snapshot component {component} has invalid effective_version")
            expected_prefix = "web" if component == "web" else "fw"
            tag = str(detail.get("tag", ""))
            if not tag.startswith(f"{expected_prefix}/v"):
                raise SnapshotError(f"Release snapshot component {component} has invalid tag")
    if not payload["release_enabled"] and payload.get("release_level") not in {"", None}:
        raise SnapshotError("Non-release snapshot must not include release_level")
    return payload


def labels_with_prefix(labels: list[dict[str, Any]], prefix: str) -> list[str]:
    return sorted(label["name"] for label in labels if isinstance(label.get("name"), str) and label["name"].startswith(prefix))


def validate_intent_labels(labels: list[dict[str, Any]], context: str) -> tuple[str, str]:
    type_labels = labels_with_prefix(labels, "type:")
    channel_labels = labels_with_prefix(labels, "channel:")
    if len(type_labels) != 1:
        raise SnapshotError(f"Expected exactly one type:* label {context}, got {type_labels or ['none']}")
    if len(channel_labels) != 1:
        raise SnapshotError(f"Expected exactly one channel:* label {context}, got {channel_labels or ['none']}")
    type_label = type_labels[0]
    channel_label = channel_labels[0]
    if type_label not in VALID_TYPES:
        raise SnapshotError(f"Unsupported type label {context}: {type_label}")
    if channel_label not in VALID_CHANNELS:
        raise SnapshotError(f"Unsupported channel label {context}: {channel_label}")
    return type_label, channel_label


def release_fields(type_label: str, channel_label: str) -> tuple[bool, str, str]:
    if type_label in {"type:docs", "type:skip"}:
        return False, "", "skip_type_label"
    level = type_label.split(":", 1)[1]
    channel = channel_label.split(":", 1)[1]
    return True, level, "pr_labels"


def parse_version(value: str) -> tuple[int, int, int]:
    match = re.fullmatch(r"([0-9]+)\.([0-9]+)\.([0-9]+)", value)
    if not match:
        raise SnapshotError(f"Invalid semver version: {value}")
    return tuple(int(part) for part in match.groups())


def render_version(version: tuple[int, int, int]) -> str:
    return ".".join(str(part) for part in version)


def bump_version(version: tuple[int, int, int], level: str) -> tuple[int, int, int]:
    major, minor, patch = version
    if level == "major":
        return major + 1, 0, 0
    if level == "minor":
        return major, minor + 1, 0
    if level == "patch":
        return major, minor, patch + 1
    raise SnapshotError(f"Unsupported release level: {level}")


def manifest_version(component: str) -> str:
    root = Path(git_output("rev-parse", "--show-toplevel"))
    if component == "web":
        payload = json.loads((root / "web/package.json").read_text(encoding="utf-8"))
        version = payload.get("version")
    elif component == "firmware":
        text = (root / "firmware/Cargo.toml").read_text(encoding="utf-8")
        match = re.search(r'(?m)^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+)"', text)
        version = match.group(1) if match else None
    else:
        raise SnapshotError(f"Unknown component: {component}")
    if not isinstance(version, str):
        raise SnapshotError(f"Failed to read {component} manifest version")
    parse_version(version)
    return version


def pending_stable_versions(notes_ref: str, target_sha: str, component: str) -> list[tuple[int, int, int]]:
    result = run_git("rev-list", "--first-parent", "--reverse", target_sha)
    versions: list[tuple[int, int, int]] = []
    for sha in result.stdout.splitlines():
        if sha == target_sha:
            continue
        payload = read_snapshot(notes_ref, sha)
        if not payload or not payload["release_enabled"] or payload["release_channel"] != "stable":
            continue
        detail = payload.get("components", {}).get(component, {})
        effective = detail.get("effective_version")
        if isinstance(effective, str):
            versions.append(parse_version(effective))
    return versions


def missing_snapshot_targets(notes_ref: str, target_sha: str) -> list[str]:
    commits = git_output("rev-list", "--first-parent", target_sha).splitlines()
    missing: list[str] = []
    for sha in commits:
        if read_snapshot(notes_ref, sha):
            break
        if sha != target_sha and stable_tag_points_at(sha):
            break
        missing.append(sha)
    return list(reversed(missing))


def max_stable_version(
    prefix: str,
    fallback: str,
    target_sha: str,
    pending_versions: list[tuple[int, int, int]],
) -> tuple[int, int, int]:
    versions = [parse_version(fallback)]
    versions.extend(pending_versions)
    for tag in git_output("tag", "--list", f"{prefix}/v[0-9]*.[0-9]*.[0-9]*").splitlines():
        match = STABLE_TAG_RE.fullmatch(tag)
        if match and match.group(1) == prefix:
            tag_sha = git_output("rev-list", "-n", "1", tag)
            if run_git("merge-base", "--is-ancestor", tag_sha, target_sha, check=False).returncode != 0:
                continue
            versions.append(tuple(int(part) for part in match.groups()[1:]))
    return max(versions)


def compute_component(
    prefix: str,
    component: str,
    level: str,
    channel: str,
    target_sha: str,
    notes_ref: str,
) -> dict[str, str]:
    base = max_stable_version(
        prefix,
        manifest_version(component),
        target_sha,
        pending_stable_versions(notes_ref, target_sha, component),
    )
    effective = render_version(bump_version(base, level))
    if channel == "rc":
        tag = f"{prefix}/v{effective}-rc.{target_sha[:7]}"
    else:
        tag = f"{prefix}/v{effective}"
    return {"effective_version": effective, "tag": tag}


def parse_intent_comment(body: str) -> dict[str, Any] | None:
    if INTENT_MARKER not in body:
        return None
    payload_text = body.split(INTENT_MARKER, 1)[1].strip()
    if payload_text.startswith("```json"):
        payload_text = payload_text.removeprefix("```json").strip()
    if payload_text.endswith("```"):
        payload_text = payload_text[:-3].strip()
    try:
        payload = json.loads(payload_text)
    except json.JSONDecodeError as exc:
        raise SnapshotError("Release intent marker comment contains invalid JSON") from exc
    if not isinstance(payload, dict):
        raise SnapshotError("Release intent marker payload must be a JSON object")
    return payload


def intent_comment_body(payload: dict[str, Any]) -> str:
    return f"{INTENT_MARKER}\n```json\n{json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=2)}\n```"


def pr_comments(api_root: str, token: str, repository: str, pr_number: int) -> list[dict[str, Any]]:
    collected: list[dict[str, Any]] = []
    page = 1
    while True:
        comments = github_json(api_root, token, repository, f"/issues/{pr_number}/comments?per_page=100&page={page}")
        if not isinstance(comments, list):
            raise SnapshotError(f"Comments payload for PR #{pr_number} page {page} must be a list")
        collected.extend(comment for comment in comments if isinstance(comment, dict))
        if len(comments) < 100:
            return collected
        page += 1


def is_trusted_intent_comment(comment: dict[str, Any]) -> bool:
    user = comment.get("user")
    if not isinstance(user, dict):
        return False
    return user.get("login") == "github-actions[bot]" and user.get("type") == "Bot"


def load_frozen_intent(api_root: str, token: str, repository: str, pr_number: int, head_sha: str) -> dict[str, str]:
    matched: list[dict[str, Any]] = []
    for comment in pr_comments(api_root, token, repository, pr_number):
        if not is_trusted_intent_comment(comment):
            continue
        body = comment.get("body")
        if not isinstance(body, str):
            continue
        payload = parse_intent_comment(body)
        if not payload:
            continue
        if payload.get("pr_number") == pr_number and payload.get("pr_head_sha") == head_sha:
            matched.append(payload)
    if not matched:
        raise SnapshotError(f"No frozen release intent marker found for PR #{pr_number} head {head_sha}")
    payload = matched[-1]
    type_label = payload.get("type_label")
    channel_label = payload.get("channel_label")
    if not isinstance(type_label, str) or not isinstance(channel_label, str):
        raise SnapshotError(f"Frozen release intent marker for PR #{pr_number} is missing labels")
    if type_label not in VALID_TYPES:
        raise SnapshotError(f"Frozen release intent marker has unsupported type label: {type_label}")
    if channel_label not in VALID_CHANNELS:
        raise SnapshotError(f"Frozen release intent marker has unsupported channel label: {channel_label}")
    return {"type_label": type_label, "channel_label": channel_label}


def parent_has_frozen_intent_gate(target_sha: str) -> bool:
    result = run_git("rev-parse", f"{target_sha}^", check=False)
    if result.returncode != 0:
        return False
    parent = result.stdout.strip()
    gate = run_git("show", f"{parent}:.github/workflows/label-gate.yml", check=False)
    if gate.returncode != 0:
        return False
    return "pull_request_target:" in gate.stdout and "capture-intent" in gate.stdout


def load_rollout_intent(api_root: str, token: str, repository: str, pr_number: int) -> dict[str, str]:
    labels = github_json(api_root, token, repository, f"/issues/{pr_number}/labels")
    if not isinstance(labels, list):
        raise SnapshotError(f"Issue labels payload for PR #{pr_number} must be a list")
    type_label, channel_label = validate_intent_labels(labels, f"on rollout PR #{pr_number}")
    return {"type_label": type_label, "channel_label": channel_label}


def write_frozen_intent(api_root: str, token: str, repository: str, payload: dict[str, Any]) -> None:
    pr_number = payload["pr_number"]
    body = intent_comment_body(payload)
    marker_comment_id = None
    for comment in pr_comments(api_root, token, repository, pr_number):
        if not is_trusted_intent_comment(comment):
            continue
        comment_body = comment.get("body")
        marker_payload = parse_intent_comment(comment_body) if isinstance(comment_body, str) else None
        if marker_payload and marker_payload.get("pr_head_sha") == payload["pr_head_sha"]:
            marker_comment_id = comment.get("id")
    if isinstance(marker_comment_id, int):
        github_request(api_root, token, repository, f"/issues/comments/{marker_comment_id}", method="PATCH", payload={"body": body})
    else:
        github_request(api_root, token, repository, f"/issues/{pr_number}/comments", method="POST", payload={"body": body})


def build_snapshot(api_root: str, token: str, repository: str, target_sha: str, notes_ref: str) -> dict[str, Any]:
    prs = github_json(api_root, token, repository, f"/commits/{target_sha}/pulls")
    if not isinstance(prs, list) or len(prs) != 1:
        raise SnapshotError(f"Expected exactly one PR associated with {target_sha}, got {len(prs) if isinstance(prs, list) else 'non-list'}")

    pr = prs[0]
    pr_number = pr.get("number")
    if not isinstance(pr_number, int):
        raise SnapshotError("Associated PR payload is missing an integer number")
    head = pr.get("head") if isinstance(pr.get("head"), dict) else {}
    head_sha = head.get("sha") if isinstance(head.get("sha"), str) else ""
    if not re.fullmatch(r"[0-9a-f]{40}", head_sha):
        raise SnapshotError(f"Associated PR #{pr_number} is missing a valid head SHA")

    try:
        intent = load_frozen_intent(api_root, token, repository, pr_number, head_sha)
        intent_source = "frozen_pr_marker"
    except SnapshotError:
        if parent_has_frozen_intent_gate(target_sha):
            raise
        intent = load_rollout_intent(api_root, token, repository, pr_number)
        intent_source = "rollout_pr_labels"
    type_label = intent["type_label"]
    channel_label = intent["channel_label"]

    release_enabled, level, reason = release_fields(type_label, channel_label)
    channel = channel_label.split(":", 1)[1]
    components: dict[str, dict[str, str]] = {}
    if release_enabled:
        run_git("fetch", "--tags", "origin")
        components = {
            "web": compute_component("web", "web", level, channel, target_sha, notes_ref),
            "firmware": compute_component("fw", "firmware", level, channel, target_sha, notes_ref),
        }
    return {
        "schema_version": SCHEMA_VERSION,
        "target_sha": target_sha,
        "snapshot_source": intent_source,
        "pr_number": pr_number,
        "pr_title": pr.get("title") if isinstance(pr.get("title"), str) else "",
        "pr_head_sha": head_sha,
        "type_label": type_label,
        "channel_label": channel_label,
        "release_enabled": release_enabled,
        "release_level": level,
        "release_channel": channel,
        "release_reason": f"frozen_{reason}",
        "components": components,
    }


def write_outputs(payload: dict[str, Any], output_path: str) -> None:
    lines = {
        "release_enabled": str(payload["release_enabled"]).lower(),
        "release_level": payload.get("release_level", ""),
        "release_channel": payload["release_channel"],
        "release_reason": payload["release_reason"],
        "type_label": payload["type_label"],
        "channel_label": payload["channel_label"],
        "pr_number": str(payload.get("pr_number") or ""),
        "pr_title": payload.get("pr_title") or "",
    }
    components = payload.get("components") if isinstance(payload.get("components"), dict) else {}
    for component, prefix in (("web", "web"), ("firmware", "fw")):
        detail = components.get(component) if isinstance(components.get(component), dict) else {}
        lines[f"{prefix}_effective_version"] = detail.get("effective_version", "")
        lines[f"{prefix}_tag"] = detail.get("tag", "")
    target = Path(output_path) if output_path else None
    text = "".join(f"{key}={value}\n" for key, value in lines.items())
    if target:
        with target.open("a", encoding="utf-8") as fh:
            fh.write(text)
    else:
        print(text, end="")


def add_note(notes_ref: str, target_sha: str, payload: dict[str, Any]) -> None:
    note = json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=2)
    run_git("notes", f"--ref={notes_ref}", "add", "-f", "-m", note, target_sha)


def push_notes_with_retry(notes_ref: str, payloads: list[tuple[str, dict[str, Any]]]) -> None:
    for attempt in range(1, 4):
        result = run_git("push", "origin", f"{notes_ref}:{notes_ref}", check=False)
        if result.returncode == 0:
            return
        if attempt == 3:
            detail = result.stderr.strip() or result.stdout.strip() or "git notes push failed"
            raise SnapshotError(detail)
        fetch_notes(notes_ref)
        for target_sha, payload in payloads:
            if read_snapshot(notes_ref, target_sha) is None:
                add_note(notes_ref, target_sha, payload)


def cmd_ensure(args: argparse.Namespace) -> None:
    if not re.fullmatch(r"[0-9a-f]{40}", args.target_sha):
        raise SnapshotError(f"Invalid target SHA: {args.target_sha}")
    run_git("cat-file", "-e", f"{args.target_sha}^{{commit}}")
    fetch_notes(args.notes_ref)
    payloads_to_push: list[tuple[str, dict[str, Any]]] = []
    for snapshot_sha in missing_snapshot_targets(args.notes_ref, args.target_sha):
        if read_snapshot(args.notes_ref, snapshot_sha) is not None:
            continue
        payload = build_snapshot(args.api_root, args.github_token, args.github_repository, snapshot_sha, args.notes_ref)
        validate_snapshot(payload, snapshot_sha)
        add_note(args.notes_ref, snapshot_sha, payload)
        payloads_to_push.append((snapshot_sha, payload))
    if payloads_to_push:
        push_notes_with_retry(args.notes_ref, payloads_to_push)

    payload = read_snapshot(args.notes_ref, args.target_sha)
    if payload is None:
        raise SnapshotError(f"Failed to materialize release snapshot for {args.target_sha}")

    Path(args.output).write_text(json.dumps(payload, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")
    print(f"Release snapshot ready for {args.target_sha}: {payload['type_label']} + {payload['channel_label']}")


def cmd_export(args: argparse.Namespace) -> None:
    fetch_notes(args.notes_ref)
    payload = read_snapshot(args.notes_ref, args.target_sha)
    if payload is None:
        raise SnapshotError(f"No release snapshot found for {args.target_sha}")
    write_outputs(payload, args.github_output)


def cmd_capture_intent(args: argparse.Namespace) -> None:
    event = json.loads(Path(args.event_path).read_text(encoding="utf-8"))
    pr = event.get("pull_request")
    if not isinstance(pr, dict):
        raise SnapshotError("capture-intent requires a pull_request event payload")
    if pr.get("state") != "open":
        print("Skip frozen release intent capture for non-open pull request.")
        return
    pr_number = pr.get("number")
    if not isinstance(pr_number, int):
        raise SnapshotError("Pull request event is missing an integer number")
    head = pr.get("head")
    head_sha = head.get("sha") if isinstance(head, dict) and isinstance(head.get("sha"), str) else ""
    if not re.fullmatch(r"[0-9a-f]{40}", head_sha):
        raise SnapshotError("Pull request event is missing a valid head SHA")
    labels = pr.get("labels")
    if not isinstance(labels, list):
        raise SnapshotError("Pull request event labels must be a list")
    type_label, channel_label = validate_intent_labels(labels, f"on PR #{pr_number}")
    release_enabled, release_level, release_reason = release_fields(type_label, channel_label)
    payload = {
        "schema_version": SCHEMA_VERSION,
        "pr_number": pr_number,
        "pr_title": pr.get("title") if isinstance(pr.get("title"), str) else "",
        "pr_head_sha": head_sha,
        "type_label": type_label,
        "channel_label": channel_label,
        "release_enabled": release_enabled,
        "release_level": release_level,
        "release_channel": channel_label.split(":", 1)[1],
        "release_reason": release_reason,
    }
    write_frozen_intent(args.api_root, args.github_token, args.github_repository, payload)
    print(f"Frozen release intent recorded for PR #{pr_number}: {type_label} + {channel_label} @ {head_sha[:7]}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Manage Flux Purr release snapshots in git notes.")
    sub = parser.add_subparsers(dest="command", required=True)

    ensure = sub.add_parser("ensure")
    ensure.add_argument("--target-sha", required=True)
    ensure.add_argument("--github-repository", required=True)
    ensure.add_argument("--github-token", required=True)
    ensure.add_argument("--notes-ref", default=DEFAULT_NOTES_REF)
    ensure.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))
    ensure.add_argument("--output", required=True)

    export = sub.add_parser("export")
    export.add_argument("--target-sha", required=True)
    export.add_argument("--notes-ref", default=DEFAULT_NOTES_REF)
    export.add_argument("--github-output", default=os.environ.get("GITHUB_OUTPUT", ""))

    capture = sub.add_parser("capture-intent")
    capture.add_argument("--event-path", default=os.environ.get("GITHUB_EVENT_PATH", ""))
    capture.add_argument("--github-repository", required=True)
    capture.add_argument("--github-token", required=True)
    capture.add_argument("--api-root", default=os.environ.get("GITHUB_API_URL", "https://api.github.com"))

    return parser.parse_args()


def main() -> int:
    try:
        args = parse_args()
        if args.command == "ensure":
            cmd_ensure(args)
        elif args.command == "export":
            cmd_export(args)
        elif args.command == "capture-intent":
            cmd_capture_intent(args)
        return 0
    except SnapshotError as exc:
        print(f"release_snapshot.py: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
