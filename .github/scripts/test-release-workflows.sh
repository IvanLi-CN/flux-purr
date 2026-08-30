#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
python3 - "${root_dir}" <<'PY'
from pathlib import Path
import sys

root = Path(sys.argv[1])
ci_pr = (root / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ci_main = (root / ".github/workflows/ci-main.yml").read_text(encoding="utf-8")
prepare = (root / ".github/workflows/release-preparation.yml").read_text(encoding="utf-8")
release = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
completion = (root / ".github/workflows/release-completion.yml").read_text(encoding="utf-8")
quality = (root / ".github/quality-gates.json").read_text(encoding="utf-8")
label_gate = (root / ".github/workflows/label-gate.yml").read_text(encoding="utf-8")
release_chain = (root / ".github/scripts/release_chain.py").read_text(encoding="utf-8")

assert "Release preparation validation" in ci_pr
assert "kind=prepared" in ci_pr
assert "needs: classify" in ci_pr
assert "prepared-integration" in ci_main
assert "git diff --quiet" in ci_main
assert "Prepare product version" in prepare
assert "workflows: [CI PR, Label Gate]" in prepare
assert "contents: write" in prepare
assert "Push prepared VERSION commit to the existing pull request" in prepare
assert "refs/heads/main" not in prepare
assert "verification_sha" in prepare
assert "verify-prepared" in prepare
assert "verify-merged-prepared" in release
assert "verify_merged_prepared_release" in release_chain
assert "verify_prepared_commit" in release_chain
assert "no_prepared_product_merge" in release_chain
assert "release/product-main" not in release
assert "Fast-forward main" not in release
assert "git push origin \"${RELEASE_SHA}:refs/heads/main\"" not in release
assert "operation=recover" not in release
assert "release_snapshot.py" not in release
assert "flux-purr-web-demo-v${PRODUCT_VERSION}.tar.gz" in release
assert "Deploy published public demo archive to EdgeOne" in release
assert "web-production-bundle" not in ci_main
assert "web-demo-bundle" not in ci_main
assert not (root / ".github/workflows/deploy-edgeone.yml").exists()
assert not (root / ".github/workflows/deploy-edgeone-demo.yml").exists()
assert "Release completion" in completion
assert "--labels-json" in completion
assert "--checks-json" in completion
assert "checks: read" in completion
assert "source-checks.json" in completion
assert "Verify VERSION and release completion" in completion
assert "Validate PR labels" in quality
assert "Release completion" in quality
assert "Validate PR labels" in label_gate
assert "capture-intent" not in label_gate
print("release workflow fixtures passed")
PY
