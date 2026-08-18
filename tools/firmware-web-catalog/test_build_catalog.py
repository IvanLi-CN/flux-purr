#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import json
import hashlib
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch


MODULE_PATH = Path(__file__).with_name("build_catalog.py")
SPEC = importlib.util.spec_from_file_location("build_catalog", MODULE_PATH)
assert SPEC and SPEC.loader
catalog = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = catalog
SPEC.loader.exec_module(catalog)


def write_bundle(path: Path, *, version: str, channel: str, source_sha: str) -> None:
    bootloader = b"boot"
    partition_table = b"\xff" * 4096
    factory_app = b"app"

    def segment(kind: str, image_path: str, address: int, image: bytes) -> dict[str, object]:
        return {
            "kind": kind,
            "path": image_path,
            "address": address,
            "length": len(image),
            "sha256": f"sha256:{hashlib.sha256(image).hexdigest()}",
            "md5": hashlib.md5(image, usedforsecurity=False).hexdigest(),  # noqa: S324
        }

    partition = segment("partition-table", "images/partition-table.bin", 0x8000, partition_table)
    manifest = {
        "schemaVersion": 1,
        "mediaType": "application/vnd.flux-purr.firmware-bundle+zip",
        "identity": {
            "version": version,
            "channel": channel,
            "sourceSha": source_sha,
            "buildId": source_sha[:16],
        },
        "target": {
            "chip": "esp32s3",
            "package": "ESP32-S3FH4R2",
            "flashSize": 4 * 1024 * 1024,
            "psramSize": 2 * 1024 * 1024,
            "flashMode": "dio",
            "flashFrequency": "40m",
        },
        "layout": {
            "id": "flux-purr.esp32s3fh4r2.factory",
            "version": 1,
            "partitionTableSha256": partition["sha256"],
        },
        "segments": [
            segment("bootloader", "images/bootloader.bin", 0, bootloader),
            partition,
            segment("factory-app", "images/factory-app.bin", 0x10000, factory_app),
        ],
        "migrations": [],
    }
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_STORED) as archive:
        archive.writestr("manifest.json", json.dumps(manifest))
        archive.writestr("images/bootloader.bin", bootloader)
        archive.writestr("images/partition-table.bin", partition_table)
        archive.writestr("images/factory-app.bin", factory_app)


class BuildFirmwareCatalogTest(unittest.TestCase):
    def args(self, root: Path, **overrides: object) -> SimpleNamespace:
        values: dict[str, object] = {
            "repo": "IvanLi-CN/flux-purr",
            "output_root": root / "web-public-firmware",
            "current_bundle": None,
            "current_tag": None,
            "current_published_at": None,
            "local_bundle": [],
            "skip_remote": True,
            "github_token_env": "GITHUB_TOKEN",
            "api_root": "https://api.github.com",
        }
        values.update(overrides)
        return SimpleNamespace(**values)

    def test_stages_local_bundle_under_exact_same_origin_path(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / "local.fluxpurr-fw"
            write_bundle(bundle, version="1.2.3-local.1", channel="local", source_sha="a" * 40)

            rendered = catalog.stage_catalog(self.args(root, local_bundle=[bundle]))

            self.assertEqual(rendered["releaseCount"], 1)
            entry = rendered["releases"][0]
            self.assertEqual(entry["source"], "local")
            self.assertEqual(entry["channel"], "local")
            self.assertTrue(entry["assetPath"].startswith("firmware/releases/local-"))
            copied = root / "web-public-firmware" / entry["assetPath"].removeprefix("firmware/")
            self.assertTrue(copied.is_file())

    def test_current_release_overrides_matching_remote_tag_and_preserves_other_releases(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            current = root / "current.fluxpurr-fw"
            older = root / "older.fluxpurr-fw"
            duplicate = root / "duplicate.fluxpurr-fw"
            write_bundle(current, version="1.4.0", channel="stable", source_sha="a" * 40)
            write_bundle(older, version="1.3.0", channel="stable", source_sha="b" * 40)
            write_bundle(duplicate, version="0.1.0", channel="stable", source_sha="c" * 40)
            releases = [
                {
                    "tag_name": "v1.4.0",
                    "published_at": "2026-08-01T00:00:00Z",
                    "draft": False,
                    "assets": [{"name": duplicate.name, "url": "https://example.test/current"}],
                },
                {
                    "tag_name": "v1.3.0",
                    "published_at": "2026-07-01T00:00:00Z",
                    "draft": False,
                    "assets": [{"name": older.name, "url": "https://example.test/older"}],
                },
            ]

            def download(url: str, output: Path, token: str | None) -> None:
                del token
                output.parent.mkdir(parents=True, exist_ok=True)
                output.write_bytes(older.read_bytes() if url.endswith("older") else duplicate.read_bytes())

            with patch.object(catalog, "list_releases", return_value=releases), patch.object(
                catalog, "download_asset", side_effect=download
            ):
                rendered = catalog.stage_catalog(
                    self.args(
                        root,
                        skip_remote=False,
                        current_bundle=current,
                        current_tag="v1.4.0",
                        current_published_at="2026-08-10T00:00:00Z",
                    )
                )

            self.assertEqual(rendered["releaseCount"], 2)
            self.assertEqual([entry["version"] for entry in rendered["releases"]], ["1.4.0", "1.3.0"])
            self.assertEqual(rendered["releases"][0]["releaseTag"], "v1.4.0")

    def test_paginates_until_github_returns_an_empty_page(self) -> None:
        responses = [[{"tag_name": "v1"}], [{"tag_name": "v2"}], []]
        with patch.object(catalog, "request_json", side_effect=responses) as request:
            releases = catalog.list_releases("IvanLi-CN/flux-purr", "https://api.github.com", None)

        self.assertEqual([release["tag_name"] for release in releases], ["v1", "v2"])
        self.assertEqual(request.call_count, 3)
        self.assertIn("page=3", request.call_args_list[-1].args[0])


if __name__ == "__main__":
    unittest.main()
