#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1


class ManifestError(RuntimeError):
    pass


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as fh:
        for chunk in iter(lambda: fh.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def canonical_sha256(value: Any) -> str:
    payload = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def read_previous(path: str | None) -> dict[str, Any] | None:
    if not path:
        return None
    previous_path = Path(path)
    if not previous_path.exists():
        return None
    payload = json.loads(previous_path.read_text(encoding="utf-8"))
    if not isinstance(payload, dict):
        raise ManifestError("Previous manifest must be a JSON object")
    return payload


def previous_components(previous: dict[str, Any] | None) -> dict[str, dict[str, Any]]:
    if not previous:
        return {}
    components = previous.get("components")
    if not isinstance(components, list):
        return {}
    return {
        str(component.get("id")): component
        for component in components
        if isinstance(component, dict) and isinstance(component.get("id"), str)
    }


def parse_component(raw: str) -> dict[str, Any]:
    try:
        component = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise ManifestError(f"Component spec is not valid JSON: {raw}") from exc
    if not isinstance(component, dict):
        raise ManifestError("Component spec must be a JSON object")
    for key in ("id", "version", "assets"):
        if key not in component:
            raise ManifestError(f"Component spec missing {key}")
    if not isinstance(component["assets"], list) or not component["assets"]:
        raise ManifestError("Component assets must be a non-empty list")
    component.setdefault("protocolVersions", [])
    return component


def build_component(
    component: dict[str, Any],
    root: Path,
    previous_by_id: dict[str, dict[str, Any]],
    source_sha: str,
) -> dict[str, Any]:
    assets = []
    for raw_path in component["assets"]:
        asset_path = root / str(raw_path)
        if not asset_path.is_file():
            raise ManifestError(f"Missing release asset: {asset_path}")
        assets.append(
            {
                "name": asset_path.name,
                "path": str(raw_path),
                "size": asset_path.stat().st_size,
                "sha256": sha256_file(asset_path),
            }
        )
    content_payload = {
        "id": component["id"],
        "version": component["version"],
        "sourceSha": source_sha,
        "protocolVersions": component.get("protocolVersions", []),
        "assets": assets,
    }
    content_sha256 = canonical_sha256(content_payload)
    previous_component = previous_by_id.get(component["id"])
    previous_sha = previous_component.get("contentSha256") if isinstance(previous_component, dict) else None
    changed = previous_sha != content_sha256
    return {
        "id": component["id"],
        "version": component["version"],
        "sourceSha": source_sha,
        "protocolVersions": component.get("protocolVersions", []),
        "assets": assets,
        "contentSha256": content_sha256,
        "changedSincePrevious": changed,
        "updateReason": "content_changed" if changed else "unchanged_since_previous",
    }


def build_manifest(args: argparse.Namespace) -> dict[str, Any]:
    previous = read_previous(args.previous_manifest)
    previous_by_id = previous_components(previous)
    root = Path(args.asset_root)
    components = [
        build_component(parse_component(raw), root, previous_by_id, args.source_sha)
        for raw in args.component
    ]
    return {
        "schemaVersion": SCHEMA_VERSION,
        "product": "flux-purr",
        "version": args.version,
        "tag": args.tag,
        "sourceSha": args.source_sha,
        "components": components,
    }


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build a Flux Purr product release manifest.")
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--asset-root", default=".")
    parser.add_argument("--previous-manifest")
    parser.add_argument("--component", action="append", required=True)
    parser.add_argument("--output", required=True)
    return parser.parse_args()


def main() -> int:
    try:
        args = parse_args()
        manifest = build_manifest(args)
        Path(args.output).write_text(json.dumps(manifest, ensure_ascii=False, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        return 0
    except ManifestError as exc:
        print(f"product_release_manifest.py: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
