#!/usr/bin/env python3

from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("product-version.py")
SPEC = importlib.util.spec_from_file_location("product_version", SCRIPT)
assert SPEC and SPEC.loader
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ProductVersionTests(unittest.TestCase):
    def write_version(self, root: Path, value: str) -> None:
        (root / "VERSION").write_text(value, encoding="utf-8")

    def test_development_uses_next_patch_and_sha_without_writing_version(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_version(root, "0.22.0\n")
            identity = MODULE.resolve(root, "development", "abcdef0123456789abcdef0123456789abcdef01")
            self.assertEqual(identity["version"], "0.22.1-dev.abcdef0")
            self.assertEqual(identity["channel"], "local")
            self.assertEqual((root / "VERSION").read_text(encoding="utf-8"), "0.22.0\n")

    def test_release_reads_exact_version_and_derives_channel(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.write_version(root, "0.22.0-rc.1\n")
            identity = MODULE.resolve(root, "release", "abcdef0123456789abcdef0123456789abcdef01")
            self.assertEqual(identity["version"], "0.22.0-rc.1")
            self.assertEqual(identity["channel"], "rc")

    def test_next_patch_handles_rc_numeric_core(self) -> None:
        self.assertEqual(MODULE.next_patch("0.22.0-rc.1"), "0.22.1")

    def test_invalid_version_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for value in ("0.1.0", "0.22.0\n\n", "0.22.0 # comment\n", "0.22.0-dev.abc\n"):
                self.write_version(root, value)
                with self.subTest(value=value), self.assertRaises(MODULE.VersionError):
                    MODULE.resolve(root, "development", "abcdef0123456789abcdef0123456789abcdef01")

    def test_missing_version_is_rejected_without_fallback(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(MODULE.VersionError):
                MODULE.resolve(Path(directory), "development", "abcdef0123456789abcdef0123456789abcdef01")


if __name__ == "__main__":
    unittest.main()
