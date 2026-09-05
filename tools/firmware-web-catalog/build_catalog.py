#!/usr/bin/env python3
"""Stage a same-origin Flux Purr firmware catalog for a Web build."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import sys
import tempfile
import zipfile
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode
from urllib.request import Request, urlopen


MAX_BUNDLE_BYTES = 8 * 1024 * 1024
API_ACCEPT = "application/vnd.github+json"
ASSET_SUFFIX = ".fluxpurr-fw"
SAFE_COMPONENT = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$")
REQUIRED_BUNDLE_ENTRIES = {
    "manifest.json",
    "images/bootloader.bin",
    "images/partition-table.bin",
    "images/factory-app.bin",
}
BUNDLE_MEDIA_TYPE = "application/vnd.flux-purr.firmware-bundle+zip"
LAYOUT_ID = "flux-purr.esp32s3fh4r2.factory"
LAYOUT_VERSION = 1
SHA256 = re.compile(r"^sha256:[0-9a-f]{64}$")
MD5 = re.compile(r"^[0-9a-f]{32}$")
SOURCE_SHA = re.compile(r"^[0-9a-f]{40}$")
BUILD_ID = re.compile(r"^[0-9a-f]{16,64}$")
VERSION = re.compile(r"^[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?$")


class CatalogError(RuntimeError):
    pass


@dataclass(frozen=True)
class BundleEntry:
    id: str
    version: str
    channel: str
    published_at: str
    source: str
    release_tag: str | None
    source_sha: str
    build_id: str
    bundle_sha256: str
    size: int
    asset_path: str

    def render(self) -> dict[str, object]:
        return {
            "id": self.id,
            "version": self.version,
            "channel": self.channel,
            "publishedAt": self.published_at,
            "source": self.source,
            "releaseTag": self.release_tag,
            "sourceSha": self.source_sha,
            "buildId": self.build_id,
            "bundleSha256": self.bundle_sha256,
            "size": self.size,
            "assetPath": self.asset_path,
            "target": "ESP32-S3FH4R2",
        }


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def sha256_bytes(value: bytes) -> str:
    return f"sha256:{hashlib.sha256(value).hexdigest()}"


def md5_bytes(value: bytes) -> str:
    return hashlib.md5(value, usedforsecurity=False).hexdigest()  # noqa: S324


def utc_now() -> str:
    return datetime.now(UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z")


def sanitize_component(value: str, label: str) -> str:
    if not SAFE_COMPONENT.fullmatch(value):
        raise CatalogError(f"{label} contains unsupported path characters")
    return value


def require_exact_keys(value: dict[str, Any], keys: set[str], label: str) -> None:
    if set(value) != keys:
        raise CatalogError(f"{label} contains missing or unknown fields")


def validate_bundle_manifest(manifest: Any, images: dict[str, bytes], path: Path) -> dict[str, str]:
    if not isinstance(manifest, dict):
        raise CatalogError(f"firmware bundle manifest is invalid: {path.name}")
    require_exact_keys(
        manifest,
        {"schemaVersion", "mediaType", "identity", "target", "layout", "segments"},
        "firmware bundle manifest",
    )
    if manifest.get("schemaVersion") != 2 or manifest.get("mediaType") != BUNDLE_MEDIA_TYPE:
        raise CatalogError(f"firmware bundle manifest header is unsupported: {path.name}")

    identity = manifest["identity"]
    target = manifest["target"]
    layout = manifest["layout"]
    segments = manifest["segments"]
    if not isinstance(identity, dict) or not isinstance(target, dict) or not isinstance(layout, dict):
        raise CatalogError(f"firmware bundle manifest is incomplete: {path.name}")
    require_exact_keys(identity, {"version", "sourceSha", "buildId", "channel"}, "firmware bundle identity")
    require_exact_keys(
        target,
        {"chip", "package", "flashSize", "psramSize", "flashMode", "flashFrequency"},
        "firmware bundle target",
    )
    require_exact_keys(layout, {"id", "version", "partitionTableSha256"}, "firmware bundle layout")
    version = identity["version"]
    source_sha = identity["sourceSha"]
    build_id = identity["buildId"]
    channel = identity["channel"]
    if (
        not isinstance(version, str)
        or not VERSION.fullmatch(version)
        or not isinstance(source_sha, str)
        or not SOURCE_SHA.fullmatch(source_sha)
        or not isinstance(build_id, str)
        or not BUILD_ID.fullmatch(build_id)
        or channel not in {"stable", "rc", "local"}
    ):
        raise CatalogError(f"firmware bundle identity is invalid: {path.name}")
    if target != {
        "chip": "esp32s3",
        "package": "ESP32-S3FH4R2",
        "flashSize": 4 * 1024 * 1024,
        "psramSize": 2 * 1024 * 1024,
        "flashMode": "dio",
        "flashFrequency": "40m",
    }:
        raise CatalogError(f"firmware bundle target is unsupported: {path.name}")
    if layout.get("id") != LAYOUT_ID or layout.get("version") != LAYOUT_VERSION:
        raise CatalogError(f"firmware bundle layout is unsupported: {path.name}")
    if not isinstance(layout.get("partitionTableSha256"), str) or not SHA256.fullmatch(layout["partitionTableSha256"]):
        raise CatalogError(f"firmware bundle layout hash is invalid: {path.name}")
    if not isinstance(segments, list) or len(segments) != 3:
        raise CatalogError(f"firmware bundle segments are invalid: {path.name}")
    expected_segments = (
        ("bootloader", "images/bootloader.bin", 0, 0x8000),
        ("partition-table", "images/partition-table.bin", 0x8000, 0x1000),
        ("factory-app", "images/factory-app.bin", 0x10000, 0x200000),
    )
    for segment, (kind, image_path, address, maximum_length) in zip(segments, expected_segments, strict=True):
        if not isinstance(segment, dict):
            raise CatalogError(f"firmware bundle segment is invalid: {path.name}")
        require_exact_keys(segment, {"kind", "path", "address", "length", "sha256", "md5"}, "firmware bundle segment")
        image = images.get(image_path)
        if (
            segment.get("kind") != kind
            or segment.get("path") != image_path
            or segment.get("address") != address
            or not isinstance(segment.get("length"), int)
            or segment["length"] < 1
            or segment["length"] > maximum_length
            or image is None
            or segment["length"] != len(image)
            or not isinstance(segment.get("sha256"), str)
            or not SHA256.fullmatch(segment["sha256"])
            or not isinstance(segment.get("md5"), str)
            or not MD5.fullmatch(segment["md5"])
            or segment["sha256"] != sha256_bytes(image)
            or segment["md5"] != md5_bytes(image)
        ):
            raise CatalogError(f"firmware bundle segment does not match its manifest: {path.name}")
    if layout["partitionTableSha256"] != segments[1]["sha256"]:
        raise CatalogError(f"firmware bundle layout hash does not match partition table: {path.name}")
    return {"version": version, "sourceSha": source_sha, "buildId": build_id, "channel": channel}


def read_bundle_metadata(path: Path) -> tuple[dict[str, str], str, int]:
    if not path.is_file():
        raise CatalogError(f"firmware bundle is missing: {path}")
    size = path.stat().st_size
    if size > MAX_BUNDLE_BYTES:
        raise CatalogError(f"firmware bundle exceeds 8 MiB: {path.name}")
    try:
        with zipfile.ZipFile(path) as archive:
            entries = archive.infolist()
            names = [entry.filename for entry in entries]
            if set(names) != REQUIRED_BUNDLE_ENTRIES or len(entries) != len(REQUIRED_BUNDLE_ENTRIES):
                raise CatalogError(f"firmware bundle file set is invalid: {path.name}")
            unpacked_size = sum(entry.file_size for entry in entries)
            if unpacked_size > MAX_BUNDLE_BYTES:
                raise CatalogError(f"firmware bundle unpacked size exceeds 8 MiB: {path.name}")
            for entry in entries:
                unix_type = (entry.external_attr >> 16) & 0o170000
                if (
                    entry.is_dir()
                    or entry.flag_bits & 0x1
                    or entry.filename.startswith("/")
                    or "\\" in entry.filename
                    or ":" in entry.filename
                    or any(part in {"", ".", ".."} for part in entry.filename.split("/"))
                    or unix_type == 0o120000
                ):
                    raise CatalogError(f"firmware bundle contains an unsafe path: {path.name}")
            manifest = json.loads(archive.read("manifest.json").decode("utf-8"))
            images = {entry: archive.read(entry) for entry in REQUIRED_BUNDLE_ENTRIES if entry != "manifest.json"}
    except (OSError, UnicodeDecodeError, zipfile.BadZipFile, json.JSONDecodeError) as error:
        raise CatalogError(f"firmware bundle cannot be read: {path.name}") from error
    return validate_bundle_manifest(manifest, images, path), sha256_file(path), size


def request_json(url: str, token: str | None) -> Any:
    request = Request(
        url,
        headers={
            "Accept": API_ACCEPT,
            "User-Agent": "flux-purr-web-firmware-catalog",
            **({"Authorization": f"Bearer {token}"} if token else {}),
        },
    )
    try:
        with urlopen(request) as response:  # noqa: S310
            return json.load(response)
    except (HTTPError, URLError, OSError, json.JSONDecodeError) as error:
        raise CatalogError(f"GitHub release catalog request failed: {error}") from error


def download_asset(url: str, output: Path, token: str | None) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    request = Request(
        url,
        headers={
            "Accept": "application/octet-stream",
            "User-Agent": "flux-purr-web-firmware-catalog",
            **({"Authorization": f"Bearer {token}"} if token else {}),
        },
    )
    try:
        with urlopen(request) as response, output.open("wb") as handle:  # noqa: S310
            while chunk := response.read(1024 * 1024):
                handle.write(chunk)
                if handle.tell() > MAX_BUNDLE_BYTES:
                    raise CatalogError(f"GitHub firmware bundle exceeds 8 MiB: {output.name}")
    except (HTTPError, URLError, OSError) as error:
        raise CatalogError(f"GitHub firmware download failed: {error}") from error


def list_releases(repo: str, api_root: str, token: str | None) -> list[dict[str, Any]]:
    owner, separator, name = repo.partition("/")
    if not separator or not owner or not name:
        raise CatalogError("--repo must use owner/repository form")
    releases: list[dict[str, Any]] = []
    page = 1
    while True:
        query = urlencode({"per_page": 100, "page": page})
        payload = request_json(f"{api_root.rstrip('/')}/repos/{owner}/{name}/releases?{query}", token)
        if not isinstance(payload, list):
            raise CatalogError("GitHub releases response is not an array")
        if not payload:
            return releases
        releases.extend(item for item in payload if isinstance(item, dict))
        page += 1


def release_asset(release: dict[str, Any]) -> tuple[str, str] | None:
    if release.get("draft") is True:
        return None
    tag = release.get("tag_name")
    published_at = release.get("published_at")
    assets = release.get("assets")
    if not isinstance(tag, str) or not isinstance(published_at, str) or not isinstance(assets, list):
        return None
    sanitize_component(tag, "release tag")
    for asset in assets:
        if not isinstance(asset, dict):
            continue
        name = asset.get("name")
        url = asset.get("url")
        if isinstance(name, str) and isinstance(url, str) and name.endswith(ASSET_SUFFIX):
            return name, url
    return None


def release_path(tag: str, bundle_sha256: str, filename: str) -> Path:
    safe_tag = sanitize_component(tag, "release tag")
    safe_file = sanitize_component(filename, "asset name")
    digest = bundle_sha256.removeprefix("sha256:")[:16]
    return Path("releases") / f"{safe_tag}-{digest}" / safe_file


def make_entry(
    *,
    metadata: dict[str, str],
    bundle_sha256: str,
    size: int,
    published_at: str,
    source: str,
    release_tag: str | None,
    asset_path: Path,
) -> BundleEntry:
    entry_id = f"{source}:{release_tag or 'local'}:{bundle_sha256.removeprefix('sha256:')[:16]}"
    return BundleEntry(
        id=entry_id,
        version=metadata["version"],
        channel=metadata["channel"],
        published_at=published_at,
        source=source,
        release_tag=release_tag,
        source_sha=metadata["sourceSha"],
        build_id=metadata["buildId"],
        bundle_sha256=bundle_sha256,
        size=size,
        asset_path=f"firmware/{asset_path.as_posix()}",
    )


def sort_entries(entries: list[BundleEntry]) -> list[BundleEntry]:
    return sorted(entries, key=lambda entry: (entry.published_at, entry.id), reverse=True)


def stage_catalog(args: argparse.Namespace) -> dict[str, object]:
    output_root = args.output_root.resolve()
    releases_root = output_root / "releases"
    output_root.mkdir(parents=True, exist_ok=True)
    staging_parent = output_root.parent
    with tempfile.TemporaryDirectory(prefix="flux-purr-web-firmware-", dir=staging_parent) as temporary:
        staging_root = Path(temporary)
        staging_releases = staging_root / "releases"
        entries: list[BundleEntry] = []
        seen_tags: set[str] = set()

        def stage_bundle(
            bundle_path: Path,
            *,
            published_at: str,
            source: str,
            release_tag: str | None,
            filename: str | None = None,
        ) -> None:
            metadata, bundle_sha256, size = read_bundle_metadata(bundle_path)
            file_name = filename or bundle_path.name
            relative = release_path(release_tag or "local", bundle_sha256, file_name)
            destination = staging_root / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(bundle_path, destination)
            entries.append(
                make_entry(
                    metadata=metadata,
                    bundle_sha256=bundle_sha256,
                    size=size,
                    published_at=published_at,
                    source=source,
                    release_tag=release_tag,
                    asset_path=relative,
                )
            )

        if args.current_bundle:
            if not args.current_tag or not args.current_published_at:
                raise CatalogError("--current-bundle requires --current-tag and --current-published-at")
            sanitize_component(args.current_tag, "current release tag")
            stage_bundle(
                args.current_bundle,
                published_at=args.current_published_at,
                source="release",
                release_tag=args.current_tag,
            )
            seen_tags.add(args.current_tag)

        for local_bundle in args.local_bundle:
            stage_bundle(
                local_bundle,
                published_at=datetime.fromtimestamp(local_bundle.stat().st_mtime, UTC)
                .replace(microsecond=0)
                .isoformat()
                .replace("+00:00", "Z"),
                source="local",
                release_tag=None,
            )

        if not args.skip_remote:
            token = os.getenv(args.github_token_env)
            releases = list_releases(args.repo, args.api_root, token)
            for release in releases:
                try:
                    asset = release_asset(release)
                    if asset is None:
                        continue
                    tag = str(release["tag_name"])
                    if tag in seen_tags:
                        continue
                    name, url = asset
                    temp_bundle = staging_root / ".downloads" / f"{tag}-{name}"
                    download_asset(url, temp_bundle, token)
                    stage_bundle(
                        temp_bundle,
                        published_at=str(release["published_at"]),
                        source="release",
                        release_tag=tag,
                        filename=name,
                    )
                except CatalogError as error:
                    print(
                        f"build_catalog.py: skipped invalid release {release.get('tag_name', '<unknown>')}: {error}",
                        file=sys.stderr,
                    )

        entries = sort_entries(entries)
        rendered = {
            "schemaVersion": 1,
            "generatedAt": utc_now(),
            "releaseCount": len(entries),
            "releases": [entry.render() for entry in entries],
        }
        manifest = staging_root / "releases-manifest.json"
        manifest.write_text(json.dumps(rendered, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        integrity = {
            "schemaVersion": 1,
            "bundles": [
                {
                    "version": entry.version,
                    "sourceSha": entry.source_sha,
                    "buildId": entry.build_id,
                    "channel": entry.channel,
                    "hardwareProfile": "ESP32-S3FH4R2",
                    "bundleSha256": entry.bundle_sha256,
                }
                for entry in entries
                if entry.source == "release" and entry.channel in {"stable", "rc"}
            ],
        }
        integrity_manifest = staging_root / "firmware-integrity-catalog.json"
        integrity_manifest.write_text(json.dumps(integrity, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        if releases_root.exists():
            shutil.rmtree(releases_root)
        if staging_releases.exists():
            shutil.move(str(staging_releases), str(releases_root))
        shutil.copy2(manifest, output_root / "releases-manifest.json")
        shutil.copy2(integrity_manifest, output_root / "firmware-integrity-catalog.json")
    return rendered


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo", default="IvanLi-CN/flux-purr")
    parser.add_argument("--output-root", type=Path, required=True)
    parser.add_argument("--current-bundle", type=Path)
    parser.add_argument("--current-tag")
    parser.add_argument("--current-published-at")
    parser.add_argument("--local-bundle", action="append", type=Path, default=[])
    parser.add_argument("--skip-remote", action="store_true")
    parser.add_argument("--github-token-env", default="GITHUB_TOKEN")
    parser.add_argument("--api-root", default="https://api.github.com")
    return parser.parse_args()


def main() -> int:
    try:
        result = stage_catalog(parse_args())
        print(json.dumps({"releaseCount": result["releaseCount"]}, sort_keys=True))
        return 0
    except CatalogError as error:
        print(f"build_catalog.py: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
