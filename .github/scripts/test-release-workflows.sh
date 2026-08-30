#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
python3 - "${root_dir}" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
ci_main = (root / ".github/workflows/ci-main.yml").read_text(encoding="utf-8")
release = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
completion = (root / ".github/workflows/release-completion.yml").read_text(encoding="utf-8")
quality = (root / ".github/quality-gates.json").read_text(encoding="utf-8")
label_gate = (root / ".github/workflows/label-gate.yml").read_text(encoding="utf-8")

assert "paths-ignore:" not in ci_main
assert "Release Commit validation" in ci_main
assert "needs: classify" in ci_main
assert "kind=release" in ci_main
assert "Release Snapshot" not in ci_main
assert "release/product-main" in release
assert "operation=recover" not in release
for token in ("release_action", "controlled_exact_required", "stage", "verify-commit", "promote", "candidate_source", "git merge-base --is-ancestor", "incomplete candidate", "flux-purr-firmware-v", "flux-purr-web-v", "flux-purr-host-tools-", "product_release_manifest.py", "Deploy published Web archive to EdgeOne", ".edgeone-deployed", "Fast-forward main"):
    assert token in release, token
assert "github.token" in release
assert "contents: write" in release
assert "github-actions[bot]" in release
assert "actions/create-github-app-token@v3" not in release
assert "RELEASE_APP_ID" not in release
assert "RELEASE_APP_PRIVATE_KEY" not in release
assert "issues: read" in release
assert "environment: product-release" not in release
assert "--mode intent" not in release
assert "release_level=" not in release
assert not (root / ".github/workflows/release-commit.yml").exists()
assert "Release completion" in completion
assert ".github/scripts/release_completion.py" in completion
assert "Validate PR labels" in quality
assert "label-gate.yml" in quality
assert "Validate PR labels" in label_gate
assert "release_snapshot.py" in label_gate
assert "release_snapshot.py" in release
assert "VERSION" in release
print("release workflow fixtures passed")
PY
