#!/usr/bin/env python3
"""Validate durable recovery assets and list only the files that must be rebuilt."""

from __future__ import annotations

import argparse
import importlib.util
import json
from pathlib import Path


VERIFY_SPEC = importlib.util.spec_from_file_location("verify_release_assets", Path(__file__).with_name("verify_release_assets.py"))
if VERIFY_SPEC is None or VERIFY_SPEC.loader is None:
    raise RuntimeError("unable to load verify_release_assets.py")
VERIFY = importlib.util.module_from_spec(VERIFY_SPEC)
VERIFY_SPEC.loader.exec_module(VERIFY)


def validate_and_list_missing(args: argparse.Namespace) -> list[str]:
    root = Path(args.asset_root).resolve()
    manifest_path = (root / args.manifest).resolve()
    if root not in manifest_path.parents:
        raise VERIFY.AssetVerificationError("recovery manifest must be inside the asset root")
    if not manifest_path.is_file():
        raise VERIFY.AssetVerificationError(f"missing durable recovery manifest: {manifest_path}")

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
            raise VERIFY.AssetVerificationError(
                f"recovery manifest {key} {payload.get(key)!r} does not match expected {value!r}"
            )

    missing: list[str] = []
    for asset in VERIFY.manifest_entries(payload, args):
        name = asset.get("name")
        relative = Path(str(asset.get("path", "")))
        if not isinstance(name, str) or relative.is_absolute() or ".." in relative.parts or relative.name != name:
            raise VERIFY.AssetVerificationError(f"recovery asset path is invalid: {relative}")
        path = root / relative
        resolved = path.resolve()
        if root not in resolved.parents:
            raise VERIFY.AssetVerificationError(f"recovery asset path escapes asset root: {relative}")
        if path.is_symlink():
            raise VERIFY.AssetVerificationError(f"recovery asset must not be a symlink: {relative}")
        if not path.is_file():
            missing.append(str(relative))
            continue
        expected_size = asset.get("size")
        expected_sha = asset.get("sha256")
        if not isinstance(expected_size, int) or not isinstance(expected_sha, str):
            raise VERIFY.AssetVerificationError(f"recovery asset metadata is invalid: {relative}")
        if path.stat().st_size != expected_size or VERIFY.sha256_file(path) != expected_sha:
            raise VERIFY.AssetVerificationError(f"existing Release asset digest mismatch: {relative}")
    return missing


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
        print("\n".join(validate_and_list_missing(args)))
    except (VERIFY.AssetVerificationError, OSError, ValueError, json.JSONDecodeError) as error:
        parser.error(str(error))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
