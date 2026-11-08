"""Exercise CI toolchain downloads and failure boundaries without real installs."""

import hashlib
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location(
    "xtensa", Path(__file__).parents[1] / "actions/setup-xtensa/toolchain.py",
)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


def write_archive(path, entries):
    with tarfile.open(path, "w:xz") as archive:
        for name, contents in entries.items():
            member = tarfile.TarInfo(name)
            member.size = len(contents)
            archive.addfile(member, io.BytesIO(contents))


class DownloadTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.payload = b"verified release asset"
        self.asset = ("esp-rs/rust-build", 397028652, "rust.tar.xz",
                      hashlib.sha256(self.payload).hexdigest(), "rust-nightly")
        self.responses = [0]
        self.calls = []

    def run_command(self, command, **options):
        self.calls.append(command)
        status = self.responses.pop(0)
        options["stdout"].write(self.payload if status == 0 else b"partial error")
        if status == "timeout":
            raise subprocess.TimeoutExpired(command, 180)
        return subprocess.CompletedProcess(command, status, stderr=f"gh: error (HTTP {status})".encode())

    def fetch(self):
        with patch.object(MODULE.subprocess, "run", side_effect=self.run_command), \
                patch.object(MODULE.time, "sleep") as sleep:
            path = MODULE.download(self.asset, self.root)
        return path, sleep

    def test_verified_official_api_download(self):
        path, _ = self.fetch()
        self.assertEqual(path.read_bytes(), self.payload)
        self.assertEqual(self.calls[0][-1], "repos/esp-rs/rust-build/releases/assets/397028652")
        self.assertFalse((self.root / "rust.tar.xz.part").exists())

    def test_504_partial_response_replaced_before_verification(self):
        self.responses = [504, 0]
        path, sleep = self.fetch()
        self.assertEqual(path.read_bytes(), self.payload)
        sleep.assert_called_once_with(5)

    def test_timeout_is_bounded_and_recoverable(self):
        self.responses = ["timeout", 0]
        path, _ = self.fetch()
        self.assertEqual(path.read_bytes(), self.payload)

    def test_retries_exhausted_clean_partial_without_publishing(self):
        self.responses = [504, 504, 504]
        with self.assertRaisesRegex(RuntimeError, "exhausted"):
            self.fetch()
        self.assertEqual(len(self.calls), 3)
        self.assertEqual(list(self.root.iterdir()), [])

    def test_permission_failure_is_not_retried(self):
        self.responses = [403]
        with self.assertRaisesRegex(RuntimeError, "official asset API failed"):
            self.fetch()
        self.assertEqual(len(self.calls), 1)
        self.assertEqual(list(self.root.iterdir()), [])

    def test_checksum_failure_preserves_existing_asset(self):
        destination = self.root / "rust.tar.xz"
        destination.write_bytes(b"previous verified download")
        self.payload = b"corrupt response"
        with self.assertRaisesRegex(ValueError, "SHA-256 mismatch"):
            self.fetch()
        self.assertEqual(destination.read_bytes(), b"previous verified download")
        self.assertFalse((self.root / "rust.tar.xz.part").exists())


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.export = self.root / "exports"
        self.events = []
        self.environment = patch.dict(os.environ, {
            "GITHUB_ACTIONS": "true", "RUNNER_ENVIRONMENT": "github-hosted",
            "RUNNER_TEMP": str(self.root),
        })
        self.environment.start()
        self.addCleanup(self.environment.stop)

    def download(self, asset, directory):
        self.events.append("download")
        path = directory / asset[2]
        entries = {asset[4] + "/install.sh": b"fixture"}
        if asset[2] == "gcc.tar.xz":
            entries = {"xtensa-esp-elf/bin/xtensa-esp-elf-gcc": b"fixture"}
        if asset[2] == "llvm.tar.xz":
            entries = {"esp-clang/lib/libclang.so": b"fixture"}
        write_archive(path, entries)
        return path

    def run_command(self, command, **options):
        self.events.append(command[0])
        return subprocess.CompletedProcess(command, 0, stdout="rustc 1.95.0.0 (fixture)")

    def install(self, **overrides):
        with patch.object(MODULE.platform, "system", return_value="Linux"), \
                patch.object(MODULE.platform, "machine", return_value="x86_64"), \
                patch.object(MODULE, "download", side_effect=overrides.get("download", self.download)), \
                patch.object(MODULE.subprocess, "run", side_effect=overrides.get("run", self.run_command)):
            MODULE.install(overrides.get("version", "1.95.0.0"), "esp32s3", self.export)

    def test_all_assets_verified_before_installer_and_exports_published_last(self):
        self.install()
        self.assertEqual(self.events[:4], ["download"] * 4)
        self.assertEqual(self.events[4:], ["bash", "bash", "rustup", "rustc"])
        self.assertIn("LIBCLANG_PATH", self.export.read_text())
        self.assertIn("xtensa-esp-elf/bin", self.export.read_text())

    def test_failed_download_prevents_install_and_export(self):
        def fail(*args):
            raise ValueError("SHA-256 mismatch")
        with self.assertRaisesRegex(ValueError, "mismatch"):
            self.install(download=fail)
        self.assertEqual(self.events, [])
        self.assertFalse(self.export.exists())

    def test_upstream_install_failure_prevents_link_and_export(self):
        def fail(command, **options):
            self.events.append(command[0])
            raise subprocess.CalledProcessError(1, command)
        with self.assertRaises(subprocess.CalledProcessError):
            self.install(run=fail)
        self.assertNotIn("rustup", self.events)
        self.assertFalse(self.export.exists())

    def test_wrong_rust_version_prevents_export(self):
        def wrong_version(command, **options):
            return subprocess.CompletedProcess(command, 0, stdout="rustc 1.94.0 (fixture)")
        with self.assertRaisesRegex(ValueError, "version mismatch"):
            self.install(run=wrong_version)
        self.assertFalse(self.export.exists())

    def test_existing_partial_install_is_never_reused_or_deleted(self):
        installation = self.root / "flux-purr-xtensa"
        installation.mkdir()
        marker = installation / "partial"
        marker.write_text("preserve")
        with self.assertRaises(FileExistsError):
            self.install()
        self.assertEqual(marker.read_text(), "preserve")
        self.assertEqual(self.events, [])

    def test_non_hosted_or_non_ci_install_rejected_before_filesystem_write(self):
        for variable in ("GITHUB_ACTIONS", "RUNNER_ENVIRONMENT"):
            with self.subTest(variable=variable), patch.dict(os.environ, {variable: "false"}):
                with self.assertRaisesRegex(RuntimeError, "GitHub-hosted"):
                    self.install()
        self.assertEqual(list(self.root.iterdir()), [])

    def test_unknown_version_rejected_before_filesystem_write(self):
        with self.assertRaisesRegex(ValueError, "unsupported"):
            self.install(version="1.96.0.0")
        self.assertEqual(list(self.root.iterdir()), [])

    def test_missing_gcc_or_libclang_prevents_success_export(self):
        def missing_binary(asset, directory):
            path = self.download(asset, directory)
            if asset[2] == "gcc.tar.xz":
                write_archive(path, {"xtensa-esp-elf/README": b"missing compiler"})
            return path
        with self.assertRaisesRegex(ValueError, "incomplete"):
            self.install(download=missing_binary)
        self.assertFalse(self.export.exists())


class ArchiveTests(unittest.TestCase):
    def test_unexpected_root_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "asset.tar.xz"
            write_archive(path, {"unexpected/bin": b"fixture"})
            with self.assertRaisesRegex(ValueError, "unexpected archive root"):
                MODULE.extract(path, root / "output", "expected")

    def test_parent_path_escape_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "asset.tar.xz"
            write_archive(path, {"expected/../../escape": b"fixture"})
            with self.assertRaises(tarfile.FilterError):
                MODULE.extract(path, root / "output", "expected")
            self.assertFalse((root / "escape").exists())


if __name__ == "__main__":
    unittest.main()
