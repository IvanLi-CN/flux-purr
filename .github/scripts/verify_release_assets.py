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
    components = payload.get("components")
    if not isinstance(components, list) or not components:
        raise AssetVerificationError("release manifest has no components")
    seen: set[Path] = set()
    for component in components:
        assets = component.get("assets") if isinstance(component, dict) else None
        if not isinstance(assets, list) or not assets:
            raise AssetVerificationError("release component has no assets")
        for asset in assets:
            if not isinstance(asset, dict):
                raise AssetVerificationError("release asset entry is not an object")
            relative = Path(str(asset.get("path", "")))
            if relative.is_absolute() or ".." in relative.parts:
                raise AssetVerificationError(f"release asset path escapes asset root: {relative}")
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
