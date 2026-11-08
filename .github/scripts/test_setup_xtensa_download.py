"""Exercise release-asset recovery without network or installing tools."""

import hashlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import zipfile

SPEC = importlib.util.spec_from_file_location(
    "xtensa_download", Path(__file__).parents[1] / "actions/setup-xtensa/download.py",
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def archive_bytes(name="espup"):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w") as archive:
        archive.writestr(name, b"fixture executable")
    return stream.getvalue()


class DownloadTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.destination = Path(self.directory.name) / "espup.zip"
        self.payload = archive_bytes()
        self.asset = {
            "name": "espup-x86_64-unknown-linux-gnu.zip", "id": 297490632,
            "digest": "sha256:" + hashlib.sha256(self.payload).hexdigest(),
        }
        self.direct_status = 0
        self.fallback_error = False
        self.transferred = None
        self.calls = []

    def run_command(self, command, **options):
        self.calls.append(command)
        if command[-1] == "repos/esp-rs/espup/releases/tags/v0.16.0":
            return subprocess.CompletedProcess(command, 0, json.dumps({"assets": [self.asset]}))
        if command[0] == "curl":
            Path(command[command.index("--output") + 1]).write_bytes(
                self.payload if self.transferred is None else self.transferred,
            )
            return subprocess.CompletedProcess(command, self.direct_status)
        self.assertEqual(command, [
            "gh", "api", "-H", "Accept: application/octet-stream",
            "repos/esp-rs/espup/releases/assets/297490632",
        ])
        if self.fallback_error:
            raise subprocess.CalledProcessError(1, command)
        options["stdout"].write(self.payload)
        return subprocess.CompletedProcess(command, 0)

    def fetch(self):
        with patch.object(MODULE.subprocess, "run", side_effect=self.run_command):
            MODULE.download("0.16.0", "x86_64-unknown-linux-gnu", self.destination)

    def assert_no_partial_file(self):
        self.assertEqual(list(Path(self.directory.name).glob("espup-download-*")), [])

    def test_direct_download_verified_before_publish(self):
        self.fetch()
        self.assertEqual(self.destination.read_bytes(), self.payload)
        self.assertEqual(len(self.calls), 2)
        self.assert_no_partial_file()

    def test_504_uses_same_asset_api_and_discards_partial_response(self):
        self.direct_status = 22
        self.transferred = b"partial 504 response"
        self.fetch()
        self.assertEqual(self.destination.read_bytes(), self.payload)
        self.assertEqual(len(self.calls), 3)
        self.assert_no_partial_file()

    def test_missing_digest_fails_before_download(self):
        del self.asset["digest"]
        with self.assertRaisesRegex(ValueError, "SHA-256"):
            self.fetch()
        self.assertEqual(len(self.calls), 1)
        self.assertFalse(self.destination.exists())

    def test_checksum_mismatch_preserves_existing_destination(self):
        self.destination.write_bytes(b"previous verified asset")
        self.transferred = b"corrupt archive"
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            self.fetch()
        self.assertEqual(self.destination.read_bytes(), b"previous verified asset")
        self.assert_no_partial_file()

    def test_verified_digest_does_not_replace_zip_validation(self):
        self.payload = b"not a zip file"
        self.asset["digest"] = "sha256:" + hashlib.sha256(self.payload).hexdigest()
        with self.assertRaises(zipfile.BadZipFile):
            self.fetch()
        self.assertFalse(self.destination.exists())
        self.assert_no_partial_file()

    def test_unexpected_archive_paths_are_rejected(self):
        self.payload = archive_bytes("../espup")
        self.asset["digest"] = "sha256:" + hashlib.sha256(self.payload).hexdigest()
        with self.assertRaisesRegex(ValueError, "unexpected"):
            self.fetch()
        self.assertFalse(self.destination.exists())
        self.assert_no_partial_file()

    def test_api_failure_never_publishes_partial_archive(self):
        self.direct_status = 22
        self.fallback_error = True
        with self.assertRaises(subprocess.CalledProcessError):
            self.fetch()
        self.assertFalse(self.destination.exists())
        self.assert_no_partial_file()


if __name__ == "__main__":
    unittest.main()
