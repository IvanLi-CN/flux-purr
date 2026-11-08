#!/usr/bin/env python3
"""Fetch one pinned espup asset, with an official API fallback and verification."""

import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import zipfile

REPOSITORY = "esp-rs/espup"


def download(version, host, destination):
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("invalid espup version")
    if not re.fullmatch(r"[a-z0-9_-]+", host):
        raise ValueError("invalid host triple")
    name = f"espup-{host}.zip"
    metadata = subprocess.run(
        ["gh", "api", f"repos/{REPOSITORY}/releases/tags/v{version}"],
        check=True, capture_output=True, text=True,
    )
    assets = [asset for asset in json.loads(metadata.stdout)["assets"]
              if asset["name"] == name]
    if len(assets) != 1:
        raise ValueError("expected exactly one matching espup release asset")
    asset = assets[0]
    digest = asset.get("digest", "")
    if not re.fullmatch(r"sha256:[a-f0-9]{64}", digest):
        raise ValueError("espup release asset has no valid SHA-256 digest")
    if type(asset.get("id")) is not int or asset["id"] <= 0:
        raise ValueError("invalid espup release asset ID")
    url = f"https://github.com/{REPOSITORY}/releases/download/v{version}/{name}"
    destination = Path(destination)
    descriptor, temporary_name = tempfile.mkstemp(
        prefix="espup-download-", dir=destination.parent,
    )
    os.close(descriptor)
    temporary = Path(temporary_name)
    try:
        result = subprocess.run([
            "curl", "--fail", "--location", "--retry", "3",
            "--connect-timeout", "30", "--max-time", "120",
            "--retry-max-time", "180", "--output", str(temporary), url,
        ])
        if result.returncode:
            print("espup release URL failed; using the official asset API.")
            with temporary.open("wb") as stream:
                subprocess.run([
                    "gh", "api", "-H", "Accept: application/octet-stream",
                    f"repos/{REPOSITORY}/releases/assets/{asset['id']}",
                ], check=True, stdout=stream)
        actual = hashlib.sha256(temporary.read_bytes()).hexdigest()
        if actual != digest.removeprefix("sha256:"):
            raise ValueError("espup release asset SHA-256 mismatch")
        with zipfile.ZipFile(temporary) as archive:
            if archive.testzip() is not None:
                raise ValueError("espup ZIP integrity check failed")
            entries = archive.infolist()
            if len(entries) != 1 or entries[0].filename != "espup":
                raise ValueError("unexpected espup ZIP contents")
        os.replace(temporary, destination)
        print(f"Verified {name}: SHA-256 {actual}")
    finally:
        temporary.unlink(missing_ok=True)


if __name__ == "__main__":
    download(*sys.argv[1:])
