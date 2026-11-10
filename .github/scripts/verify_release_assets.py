#!/usr/bin/env python3
"""Verify a downloaded or freshly built release asset set against its identity manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path


class AssetVerificationError(RuntimeError):
    pass


def canonical_sha256(value: object) -> str:
    payload = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def required_asset_names(version: str) -> dict[str, set[str]]:
    return {
        "web": {
            f"flux-purr-web-v{version}.tar.gz",
            f"flux-purr-web-demo-v{version}.tar.gz",
        },
        "firmware": {
            f"flux-purr-firmware-v{version}.fluxpurr-fw",
            f"flux-purr-firmware-v{version}.tar.gz",
        },
        "host-tools": {f"flux-purr-host-tools-linux-x64-v{version}.tar.gz"},
    }


def manifest_entries(payload: dict[str, object], args: argparse.Namespace) -> list[dict[str, object]]:
    if payload.get("product") != "flux-purr":
        raise AssetVerificationError("release manifest has an unexpected product")
    if payload.get("schemaVersion") != 1:
        raise AssetVerificationError("release manifest has an unsupported schema version")
    components = payload.get("components")
    if not isinstance(components, list):
        raise AssetVerificationError("release manifest components must be a list")
    expected_components = required_asset_names(args.version)
    expected_channel = "rc" if "-rc." in args.version else "stable"
    if payload.get("channel") != expected_channel:
        raise AssetVerificationError("release manifest has an invalid channel")
    by_id: dict[str, dict[str, object]] = {}
    entries: list[dict[str, object]] = []
    for component in components:
        if not isinstance(component, dict):
            raise AssetVerificationError("release component is not an object")
        component_id = component.get("id")
        if not isinstance(component_id, str) or component_id in by_id:
            raise AssetVerificationError("release manifest has duplicate or invalid component ids")
        if component_id not in expected_components:
            raise AssetVerificationError(f"release manifest has unexpected component: {component_id}")
        if component.get("version") != args.version or component.get("sourceSha") != args.source_sha:
            raise AssetVerificationError(f"release component {component_id} is not bound to this identity")
        protocol_versions = component.get("protocolVersions")
        if not isinstance(protocol_versions, list) or not all(isinstance(value, str) for value in protocol_versions):
            raise AssetVerificationError(f"release component {component_id} has invalid protocol versions")
        content_sha = component.get("contentSha256")
        if not isinstance(content_sha, str):
            raise AssetVerificationError(f"release component {component_id} has no content digest")
        assets = component.get("assets")
        if not isinstance(assets, list) or not assets:
            raise AssetVerificationError(f"release component {component_id} has no assets")
        names: set[str] = set()
        for asset in assets:
            if not isinstance(asset, dict):
                raise AssetVerificationError("release asset entry is not an object")
            name = asset.get("name")
            if not isinstance(name, str) or name in names:
                raise AssetVerificationError(f"release component {component_id} has duplicate asset names")
            if asset.get("channel") != expected_channel:
                raise AssetVerificationError(f"release asset {name} has an invalid channel")
            names.add(name)
            entries.append(asset)
        if not expected_components[component_id].issubset(names):
            missing = sorted(expected_components[component_id] - names)
            raise AssetVerificationError(f"release component {component_id} is missing assets: {missing}")
        content_payload = {
            "id": component_id,
            "version": args.version,
            "sourceSha": args.source_sha,
            "protocolVersions": protocol_versions,
            "assets": assets,
        }
        if content_sha != canonical_sha256(content_payload):
            raise AssetVerificationError(f"release component {component_id} content digest is invalid")
        changed = component.get("changedSincePrevious")
        reason = component.get("updateReason")
        if not isinstance(changed, bool) or reason not in {"content_changed", "unchanged_since_previous"}:
            raise AssetVerificationError(f"release component {component_id} has invalid change metadata")
        if (changed and reason != "content_changed") or (not changed and reason != "unchanged_since_previous"):
            raise AssetVerificationError(f"release component {component_id} change metadata is inconsistent")
        by_id[component_id] = component
    if set(by_id) != set(expected_components):
        missing = sorted(set(expected_components) - set(by_id))
        raise AssetVerificationError(f"release manifest is missing components: {missing}")
    return entries


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify(args: argparse.Namespace) -> None:
    root = Path(args.asset_root).resolve()
    manifest_path = (root / args.manifest).resolve()
    if root not in manifest_path.parents:
        raise AssetVerificationError("manifest must be inside the asset root")
    if not manifest_path.is_file():
        raise AssetVerificationError(f"missing release manifest: {manifest_path}")
    payload = json.loads(manifest_path.read_text(encoding="utf-8"))
    expected = {
        "version": args.version,
        "tag": args.tag,
        "sourceSha": args.source_sha,
        "releaseIntent": args.intent,
        "trustedBaseline": args.baseline,
        "reservationId": args.reservation_id,
    }
    for key, value in expected.items():
        if payload.get(key) != value:
            raise AssetVerificationError(
                f"manifest {key} {payload.get(key)!r} does not match expected {value!r}"
            )
    entries = manifest_entries(payload, args)
    seen: set[Path] = set()
    for asset in entries:
        relative = Path(str(asset.get("path", "")))
        name = asset.get("name")
        if relative.is_absolute() or ".." in relative.parts or relative.name != name:
            raise AssetVerificationError(f"release asset path is invalid: {relative}")
        path = (root / relative).resolve()
        if root not in path.parents:
            raise AssetVerificationError(f"release asset path escapes asset root: {relative}")
        if path in seen:
            raise AssetVerificationError(f"release asset listed more than once: {relative}")
        seen.add(path)
        if not path.is_file():
            raise AssetVerificationError(f"missing release asset: {relative}")
        expected_size = asset.get("size")
        if expected_size != path.stat().st_size:
            raise AssetVerificationError(f"release asset size mismatch: {relative}")
        expected_sha = asset.get("sha256")
        if not isinstance(expected_sha, str) or sha256_file(path) != expected_sha:
            raise AssetVerificationError(f"release asset digest mismatch: {relative}")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--asset-root", required=True)
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--intent", choices=("patch", "minor", "major", "rc"), required=True)
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--reservation-id", required=True)
    args = parser.parse_args()
    try:
        verify(args)
    except (AssetVerificationError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"verify_release_assets.py: {error}", file=sys.stderr)
        return 1
    print("release assets verified")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
