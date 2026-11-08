#!/usr/bin/env python3
"""Install the pinned upstream toolchain through verified official asset APIs."""

from concurrent.futures import ThreadPoolExecutor
import hashlib
import os
from pathlib import Path
import platform
import re
import shlex
import subprocess
import sys
import tarfile
import tempfile
import time

# Exact assets selected by espup v0.16.0 for Rust 1.95.0.0 on hosted Linux.
# Upstream installers/layout: esp-rs/espup v0.16.0 src/toolchain/{rust,gcc,llvm}.rs.
# Digests and asset IDs are bound to the official release metadata, not mirrors.
ASSETS = (
    ("esp-rs/rust-build", 397028652, "rust.tar.xz",
     "aad2fb24baeab6ad61c41f002f136fe0b416ef39a501d29750ab66c18a699433",
     "rust-nightly-x86_64-unknown-linux-gnu"),
    ("esp-rs/rust-build", 396979378, "rust-src.tar.xz",
     "708bee337ac2d41c0e861af93047bffec91a052fbcb4957925d840a41f327717",
     "rust-src-nightly"),
    ("espressif/crosstool-NG", 295390818, "gcc.tar.xz",
     "e3d77ad14544814527bbe7a2d0f79ec4592a4e23392c51c7388c0e686b6a6977",
     "xtensa-esp-elf"),
    ("espressif/llvm-project", 288862470, "llvm.tar.xz",
     "0128e2a86b262e3edfce026034ede07e2f45732b10c6cb8ac522c8f4c72befd8",
     "esp-clang"),
)


def download(asset, directory):
    repository, asset_id, name, digest, _ = asset
    destination = directory / name
    temporary = directory / (name + ".part")
    try:
        for attempt in range(3):
            try:
                with temporary.open("wb") as output:
                    result = subprocess.run([
                        "gh", "api", "-H", "Accept: application/octet-stream",
                        f"repos/{repository}/releases/assets/{asset_id}",
                    ], stdout=output, stderr=subprocess.PIPE, timeout=180)
                if result.returncode == 0:
                    break
                error = result.stderr.decode(errors="replace")
                if not re.search(r"HTTP (408|429|500|502|503|504)|connection|timeout|timed out", error, re.I):
                    raise RuntimeError(f"official asset API failed for {name} (exit {result.returncode})")
            except subprocess.TimeoutExpired:
                pass
            if attempt == 2:
                raise RuntimeError(f"official asset API retries exhausted for {name}")
            time.sleep((5, 15)[attempt])
        actual = hashlib.sha256()
        with temporary.open("rb") as stream:
            for chunk in iter(lambda: stream.read(1024 * 1024), b""):
                actual.update(chunk)
        if actual.hexdigest() != digest:
            raise ValueError(f"SHA-256 mismatch for {name}")
        os.replace(temporary, destination)
        print(f"Verified {name}: SHA-256 {digest}", flush=True)
        return destination
    finally:
        temporary.unlink(missing_ok=True)


def extract(archive_path, destination, expected_root):
    with tarfile.open(archive_path, "r:xz") as archive:
        members = archive.getmembers()
        if not members or any(Path(member.name).parts[0] != expected_root for member in members):
            raise ValueError(f"unexpected archive root in {archive_path.name}")
        # Reject path/link escapes even for a checksum-verified upstream archive.
        archive.extractall(destination, members=members, filter="data")


def install(version, targets, export_file):
    if (os.environ.get("GITHUB_ACTIONS") != "true"
            or os.environ.get("RUNNER_ENVIRONMENT") != "github-hosted"):
        raise RuntimeError("Xtensa installation requires a disposable GitHub-hosted job")
    if (version, targets, platform.system(), platform.machine()) != (
        "1.95.0.0", "esp32s3", "Linux", "x86_64",
    ):
        raise ValueError("unsupported pinned Xtensa toolchain configuration")
    runner_temp = Path(os.environ["RUNNER_TEMP"]).resolve(strict=True)
    root = runner_temp / "flux-purr-xtensa"
    root.mkdir()  # Never reuse or remove a partial/existing installation.
    esp = root / "esp"
    with tempfile.TemporaryDirectory(prefix="xtensa-assets-", dir=runner_temp) as temporary:
        scratch = Path(temporary)
        with ThreadPoolExecutor(max_workers=4) as executor:
            archives = list(executor.map(lambda asset: download(asset, scratch), ASSETS))
        # All assets must verify before executing any upstream installer.
        for asset, archive in zip(ASSETS, archives):
            extract(archive, root, asset[4])
        for component in (ASSETS[0][4], ASSETS[1][4]):
            subprocess.run([
                "bash", str(root / component / "install.sh"), f"--prefix={esp}",
                "--without=rust-docs-json-preview,rust-docs", "--disable-ldconfig",
            ], check=True)
    subprocess.run(["rustup", "toolchain", "link", "esp", str(esp)], check=True)
    result = subprocess.run(["rustc", "+esp", "--version"], check=True, capture_output=True, text=True)
    if not re.search(r"\brustc 1\.95\.0(?:\.0)?(?:[ -])", result.stdout):
        raise ValueError("installed Xtensa Rust version mismatch")
    library_path = root / "esp-clang/lib"
    gcc_bin = root / "xtensa-esp-elf/bin"
    if not (library_path / "libclang.so").exists() or not (gcc_bin / "xtensa-esp-elf-gcc").is_file():
        raise ValueError("incomplete Xtensa LLVM/GCC installation")
    Path(export_file).write_text(
        f"export LIBCLANG_PATH={shlex.quote(str(library_path))}\n"
        f"export PATH={shlex.quote(str(gcc_bin))}:\"$PATH\"\n",
    )


if __name__ == "__main__":
    install(*sys.argv[1:])
