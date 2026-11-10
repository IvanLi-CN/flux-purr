#!/usr/bin/env python3
"""Reconcile a partial immutable Release with freshly rebuilt missing assets."""

from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
from pathlib import Path

VERIFY_SPEC = importlib.util.spec_from_file_location("verify_release_assets", Path(__file__).with_name("verify_release_assets.py"))
if VERIFY_SPEC is None or VERIFY_SPEC.loader is None:
    raise RuntimeError("unable to load verify_release_assets.py")
VERIFY = importlib.util.module_from_spec(VERIFY_SPEC)
VERIFY_SPEC.loader.exec_module(VERIFY)


def reconcile(args: argparse.Namespace) -> None:
    existing_root = Path(args.existing_root).resolve()
    built_root = Path(args.built_root).resolve()
    manifest_path = existing_root / args.manifest
    manifest_from_existing = manifest_path.is_file()
    if not manifest_from_existing:
        if not args.allow_missing_manifest:
            raise VERIFY.AssetVerificationError("partial Release has no durable manifest")
        manifest_path = built_root / args.manifest
        if not manifest_path.is_file():
            raise VERIFY.AssetVerificationError("rebuilt Release has no canonical manifest")
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
                f"partial Release manifest {key} {payload.get(key)!r} does not match expected {value!r}"
            )
    entries = VERIFY.manifest_entries(payload, args)
    missing: list[str] = []
    for asset in entries:
        relative = Path(str(asset["path"]))
        existing = (existing_root / relative).resolve()
        built = (built_root / relative).resolve()
        if existing_root not in existing.parents or built_root not in built.parents:
            raise VERIFY.AssetVerificationError(f"release asset path escapes its root: {relative}")
        expected_size = asset.get("size")
        expected_sha = asset.get("sha256")
        if not isinstance(expected_size, int) or not isinstance(expected_sha, str):
            raise VERIFY.AssetVerificationError(f"release asset metadata is invalid: {relative}")
        if existing.is_file():
            if existing.stat().st_size != expected_size or VERIFY.sha256_file(existing) != expected_sha:
                raise VERIFY.AssetVerificationError(f"existing Release asset digest mismatch: {relative}")
            built.parent.mkdir(parents=True, exist_ok=True)
            if built.exists():
                built.unlink()
            shutil.copy2(existing, built)
        else:
            if not built.is_file() or built.stat().st_size != expected_size or VERIFY.sha256_file(built) != expected_sha:
                raise VERIFY.AssetVerificationError(f"rebuilt missing asset does not match manifest: {relative}")
            missing.append(str(Path("release-assets") / relative))
    built_manifest = built_root / args.manifest
    if manifest_from_existing:
        shutil.copy2(manifest_path, built_manifest)
    else:
        missing.append(str(Path("release-assets") / args.manifest))
    for marker in (".edgeone-deploying", ".edgeone-deployed", ".edgeone-demo-deploying", ".edgeone-demo-deployed"):
        source = existing_root / marker
        if source.is_file():
            shutil.copy2(source, built_root / marker)
    Path(args.upload_list).write_text("\n".join(missing) + ("\n" if missing else ""), encoding="utf-8")
    VERIFY.verify(
        argparse.Namespace(
            asset_root=str(built_root),
            manifest=args.manifest,
            version=args.version,
            tag=args.tag,
            source_sha=args.source_sha,
            intent=args.intent,
            baseline=args.baseline,
            reservation_id=args.reservation_id,
        )
    )


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--existing-root", required=True)
    parser.add_argument("--built-root", required=True)
    parser.add_argument("--manifest", required=True)
    parser.add_argument("--version", required=True)
    parser.add_argument("--tag", required=True)
    parser.add_argument("--source-sha", required=True)
    parser.add_argument("--intent", choices=("patch", "minor", "major", "rc"), required=True)
    parser.add_argument("--baseline", required=True)
    parser.add_argument("--reservation-id", required=True)
    parser.add_argument("--upload-list", required=True)
    parser.add_argument("--allow-missing-manifest", action="store_true")
    args = parser.parse_args()
    try:
        reconcile(args)
    except (VERIFY.AssetVerificationError, OSError, ValueError, json.JSONDecodeError) as error:
        print(f"reconcile_release_assets.py: {error}")
        return 1
    print("partial release assets reconciled")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
