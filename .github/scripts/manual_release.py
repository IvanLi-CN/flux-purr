#!/usr/bin/env python3
"""Resolve and validate Flux Purr manual release identities.

The release target is an owner-confirmed intent.  The canonical version is
derived here from the trusted stable baseline and the immutable tag set; it is
never accepted from a workflow caller as an arbitrary version string.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Iterable

ROOT = Path(__file__).resolve().parents[2]
VERSION_SPEC = importlib.util.spec_from_file_location("product_version", ROOT / "scripts/product-version.py")
if VERSION_SPEC is None or VERSION_SPEC.loader is None:
    raise RuntimeError("unable to load product-version.py")
VERSION = importlib.util.module_from_spec(VERSION_SPEC)
VERSION_SPEC.loader.exec_module(VERSION)


class ManualReleaseError(RuntimeError):
    pass


INTENTS = {"patch", "minor", "major", "rc"}
TAG_RE = re.compile(r"^v(?P<version>\d+\.\d+\.\d+(?:-rc\.\d+)?)$")
DOC_EXTENSIONS = {".md", ".markdown", ".txt", ".rst"}


def parse_tags(path: Path | None) -> dict[str, str]:
    if path is None:
        return {}
    result: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) != 2:
            raise ManualReleaseError(f"invalid tag inventory row: {line!r}")
        tag, sha = parts
        if not re.fullmatch(r"[0-9a-f]{40}", sha):
            raise ManualReleaseError(f"tag {tag} has invalid target SHA")
        result[tag] = sha
    return result


def parse_intents(path: Path | None) -> dict[str, str]:
    if path is None:
        return {}
    result: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) != 2:
            raise ManualReleaseError(f"invalid tag intent row: {line!r}")
        tag, intent = parts
        if intent not in INTENTS:
            raise ManualReleaseError(f"tag {tag} has unsupported release intent: {intent}")
        if tag in result:
            raise ManualReleaseError(f"tag {tag} has duplicate release intent metadata")
        result[tag] = intent
    return result


def parse_baselines(path: Path | None) -> dict[str, str]:
    if path is None:
        return {}
    result: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) != 2:
            raise ManualReleaseError(f"invalid tag baseline row: {line!r}")
        tag, baseline = parts
        try:
            parsed = VERSION.parse_version(baseline)
        except VERSION.VersionError as error:
            raise ManualReleaseError(f"tag {tag} has invalid trusted baseline: {baseline}") from error
        if parsed["rc"] is not None:
            raise ManualReleaseError(f"tag {tag} trusted baseline must be stable: {baseline}")
        if tag in result:
            raise ManualReleaseError(f"tag {tag} has duplicate trusted baseline metadata")
        result[tag] = baseline
    return result


def parse_reservations(path: Path | None) -> dict[str, str]:
    if path is None:
        return {}
    result: dict[str, str] = {}
    for line in path.read_text(encoding="utf-8").splitlines():
        parts = line.split()
        if len(parts) != 2:
            raise ManualReleaseError(f"invalid tag reservation row: {line!r}")
        tag, reservation = parts
        if not re.fullmatch(r"v\d+\.\d+\.\d+(?:-rc\.\d+)?@[0-9a-f]{40}", reservation):
            raise ManualReleaseError(f"tag {tag} has invalid reservation metadata: {reservation}")
        if tag in result:
            raise ManualReleaseError(f"tag {tag} has duplicate reservation metadata")
        result[tag] = reservation
    return result


def version_tuple(value: str) -> tuple[int, int, int, int]:
    parsed = VERSION.parse_version(value)
    return (
        int(parsed["major"]),
        int(parsed["minor"]),
        int(parsed["patch"]),
        int(parsed["rc"] or 0),
    )


def stable_base(value: str) -> str:
    parsed = VERSION.parse_version(value)
    return f"{parsed['major']}.{parsed['minor']}.{parsed['patch']}"


def validate_stable_baseline(value: str) -> None:
    parsed = VERSION.parse_version(value)
    if parsed["rc"] is not None:
        raise ManualReleaseError(f"trusted baseline must be stable: {value}")


def next_stable(value: str, intent: str) -> str:
    parsed = VERSION.parse_version(stable_base(value))
    major, minor, patch = int(parsed["major"]), int(parsed["minor"]), int(parsed["patch"])
    if intent == "patch":
        patch += 1
    elif intent == "minor":
        minor += 1
        patch = 0
    elif intent == "major":
        major += 1
        minor = 0
        patch = 0
    else:
        raise ManualReleaseError(f"unsupported stable release intent: {intent}")
    return f"{major}.{minor}.{patch}"


def latest_stable(tags: dict[str, str], source_sha: str | None = None) -> tuple[str, str] | None:
    candidates: list[tuple[tuple[int, int, int, int], str, str]] = []
    for tag, sha in tags.items():
        match = TAG_RE.fullmatch(tag)
        if not match or "-rc." in match.group("version"):
            continue
        if source_sha and not is_ancestor(sha, source_sha):
            continue
        version = match.group("version")
        candidates.append((version_tuple(version), tag, sha))
    if not candidates:
        return None
    _, tag, sha = max(candidates)
    return tag, sha


def is_ancestor(ancestor: str, descendant: str) -> bool:
    result = subprocess.run(
        ["git", "merge-base", "--is-ancestor", ancestor, descendant],
        cwd=ROOT,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    return result.returncode == 0


def resolve_version(intent: str, baseline: str, tags: dict[str, str]) -> str:
    if intent not in INTENTS:
        raise ManualReleaseError(f"unsupported release intent: {intent}")
    validate_stable_baseline(baseline)
    base = next_stable(baseline, intent if intent != "rc" else "patch")
    if intent != "rc":
        return base
    prefix = f"v{base}-rc."
    numbers = []
    for tag in tags:
        if tag.startswith(prefix):
            suffix = tag[len(prefix) :]
            if suffix.isdigit() and int(suffix) > 0:
                numbers.append(int(suffix))
    return f"{base}-rc.{max(numbers, default=0) + 1}"


def document_only(paths: Iterable[str]) -> bool:
    names = [path for path in paths if path]
    if not names:
        return True
    for path in names:
        normalized = path.lstrip("./")
        name = Path(normalized).name
        suffix = Path(normalized).suffix.lower()
        if normalized.startswith("docs/"):
            continue
        if name.startswith(("README", "CHANGELOG")):
            continue
        if suffix in DOC_EXTENSIONS:
            continue
        return False
    return True


def release_required(paths: Iterable[str]) -> bool:
    return not document_only(paths)


def matching_tag(tags: dict[str, str], source_sha: str) -> tuple[str, str] | None:
    matches = [(tag, sha) for tag, sha in tags.items() if sha == source_sha and TAG_RE.fullmatch(tag)]
    if len(matches) > 1:
        raise ManualReleaseError(f"source SHA has multiple release tags: {sorted(tag for tag, _ in matches)}")
    return matches[0] if matches else None


def identity(
    intent: str,
    baseline: str,
    source_sha: str,
    tags: dict[str, str],
    operation: str,
    tag_intents: dict[str, str] | None = None,
    tag_baselines: dict[str, str] | None = None,
    tag_reservations: dict[str, str] | None = None,
) -> dict[str, str]:
    if not re.fullmatch(r"[0-9a-f]{40}", source_sha):
        raise ManualReleaseError("source SHA must be a full lowercase hexadecimal commit SHA")
    validate_stable_baseline(baseline)
    if operation == "recover":
        existing = matching_tag(tags, source_sha)
        if existing is None:
            raise ManualReleaseError("recovery requires an existing release tag pointing to the exact source SHA")
        tag, target = existing
        version = TAG_RE.fullmatch(tag).group("version")  # type: ignore[union-attr]
        reserved_intent = (tag_intents or {}).get(tag)
        if reserved_intent is None:
            raise ManualReleaseError(f"release tag {tag} has no durable release intent metadata")
        if reserved_intent != intent:
            raise ManualReleaseError(
                f"recovery intent {intent} does not match reserved release intent {reserved_intent} for {tag}"
            )
        reserved_baseline = (tag_baselines or {}).get(tag)
        if reserved_baseline is None:
            raise ManualReleaseError(f"release tag {tag} has no durable trusted baseline metadata")
        expected_reservation = f"{tag}@{target}"
        reserved_reservation = (tag_reservations or {}).get(tag)
        if reserved_reservation is None:
            raise ManualReleaseError(f"release tag {tag} has no durable reservation metadata")
        if reserved_reservation != expected_reservation:
            raise ManualReleaseError(
                f"release tag {tag} reservation {reserved_reservation} does not match {expected_reservation}"
            )
        if intent == "rc" and "-rc." not in version:
            raise ManualReleaseError("rc recovery requires an RC tag")
        if intent != "rc" and "-rc." in version:
            raise ManualReleaseError("stable recovery cannot use an RC tag")
        return {
            "intent": intent,
            "baseline": reserved_baseline,
            "version": version,
            "tag": tag,
            "source_sha": target,
            "reservation_id": f"{tag}@{target}",
            "reservation_status": "matching",
            "operation": "recover",
        }
    existing_source = matching_tag(tags, source_sha)
    if existing_source is not None:
        raise ManualReleaseError(
            f"source SHA already has release tag {existing_source[0]}; use same-identity recovery"
        )
    version = resolve_version(intent, baseline, tags)
    tag = f"v{version}"
    owner = tags.get(tag)
    if owner is not None:
        raise ManualReleaseError(f"release tag {tag} is already owned by {owner}; use same-identity recovery")
    return {
        "intent": intent,
        "baseline": baseline,
        "version": version,
        "tag": tag,
        "source_sha": source_sha,
        "reservation_id": f"{tag}@{source_sha}",
        "reservation_status": "available",
        "operation": "publish",
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    baseline = sub.add_parser("baseline")
    baseline.add_argument("--tags-file", type=Path, required=True)
    baseline.add_argument("--source-sha")
    baseline.add_argument("--fallback-version", required=True)
    resolve = sub.add_parser("resolve")
    resolve.add_argument("--intent", choices=sorted(INTENTS), required=True)
    resolve.add_argument("--baseline", required=True)
    resolve.add_argument("--tags-file", type=Path, required=True)
    classify = sub.add_parser("classify")
    classify.add_argument("--paths-file", type=Path, required=True)
    ident = sub.add_parser("identity")
    ident.add_argument("--intent", choices=sorted(INTENTS), required=True)
    ident.add_argument("--baseline", required=True)
    ident.add_argument("--source-sha", required=True)
    ident.add_argument("--tags-file", type=Path, required=True)
    ident.add_argument("--intent-file", type=Path)
    ident.add_argument("--baseline-file", type=Path)
    ident.add_argument("--reservation-file", type=Path)
    ident.add_argument("--operation", choices=("publish", "recover"), default="publish")
    args = parser.parse_args(argv)
    try:
        if args.command == "baseline":
            tags = parse_tags(args.tags_file)
            validate_stable_baseline(args.fallback_version)
            result = latest_stable(tags, args.source_sha)
            print(json.dumps({"tag": result[0], "version": result[0][1:], "sha": result[1]} if result else {"tag": "", "version": args.fallback_version, "sha": ""}, sort_keys=True))
        elif args.command == "resolve":
            print(json.dumps({"intent": args.intent, "version": resolve_version(args.intent, args.baseline, parse_tags(args.tags_file))}, sort_keys=True))
        elif args.command == "classify":
            paths = args.paths_file.read_text(encoding="utf-8").splitlines()
            print(json.dumps({"document_only": document_only(paths), "release_required": release_required(paths), "paths": paths}, sort_keys=True))
        else:
            print(
                json.dumps(
                    identity(
                        args.intent,
                        args.baseline,
                        args.source_sha,
                        parse_tags(args.tags_file),
                        args.operation,
                        parse_intents(args.intent_file),
                        parse_baselines(args.baseline_file),
                        parse_reservations(args.reservation_file),
                    ),
                    sort_keys=True,
                )
            )
        return 0
    except (ManualReleaseError, VERSION.VersionError, OSError, ValueError) as error:
        print(f"manual_release.py: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
