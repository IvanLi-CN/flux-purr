#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
bash "${root_dir}/.github/scripts/test-edgeone-deploy.sh"
python3 -m unittest discover -s "${root_dir}/.github/scripts" -p 'test_setup_xtensa*.py'
python3 - "${root_dir}" <<'PY'
from pathlib import Path
import json
import sys

root = Path(sys.argv[1])
release = (root / ".github/workflows/release.yml").read_text(encoding="utf-8")
ci_pr = (root / ".github/workflows/ci.yml").read_text(encoding="utf-8")
ci_main = (root / ".github/workflows/ci-main.yml").read_text(encoding="utf-8")
notify = (root / ".github/workflows/notify-release-failure.yml").read_text(encoding="utf-8")
quality = json.loads((root / ".github/quality-gates.json").read_text(encoding="utf-8"))
edgeone_helper = (root / ".github/scripts/deploy-edgeone.sh").read_text(encoding="utf-8")

assert "workflow_dispatch:" in release
assert "workflow_run:" not in release
assert "group: release-product-main" in release
assert "options: [patch, minor, major, rc]" in release
assert "options: [publish, recover]" in release
assert "manual_release.py baseline" in release
assert "manual_release.py classify" in release
assert "manual_release.py identity" in release
assert "git rev-parse origin/main" in release
assert 'ref: ${{ github.sha }}' in release
assert 'RELEASE_SOURCE_SHA: ${{ github.sha }}' in release
assert "main advanced after dispatch" in release
assert "git for-each-ref" in release
assert "Flux-Purr-Release-Intent:" in release
assert "--intent-file" in release
assert "--baseline-file" in release
assert "--reservation-file" in release
assert "Flux-Purr-Trusted-Baseline:" in release
assert "Flux-Purr-Reservation-ID:" in release
assert "identity_baseline" in release
assert 'if [ "${release_required}" != true ] && [ "${RELEASE_OPERATION}" != recover ]; then' in release
assert "Recovery requested; continue to verify the existing release identity" in release
assert "Reserve immutable release tag" in release
assert "git push origin \"${PRODUCT_TAG}\"" in release
assert "Checkout release source and bind VERSION" in release
assert "printf '%s\\n' \"${PRODUCT_VERSION}\" > VERSION" in release
assert "gh release create" in release
assert "Restore immutable release assets for recovery" in release
assert "gh release download" in release
assert "verify_release_assets.py" in release
assert "reconcile_release_assets.py" in release
assert "status=missing" in release
assert "status=partial" in release
assert "release-upload-list.txt" in release
assert "release-missing-assets.txt" in release
assert "asset_missing()" in release
assert "gh release create \"${PRODUCT_TAG}\" \"${manifest}\"" in release
assert "gh release upload \"${PRODUCT_TAG}\" \"${upload_assets[@]}\"" in release
assert "already exists; publish cannot mutate an existing identity" in release
assert "--clobber" not in release
assert "Deploy published Web archive to EdgeOne" in release
assert "Deploy published public demo archive to EdgeOne" in release
assert "marker belongs to" in release
assert "deployment-started marker" in release
assert "operator review is required before retry" in release
assert release.index('gh release upload "${PRODUCT_TAG}" "${started_marker_path}"') < release.index('bash "${RUNNER_TEMP}/flux-purr-deploy-edgeone.sh"')
assert "release_chain.py" not in release
assert "release_snapshot.py" not in release
assert "commit_sha" not in release
assert "label" not in release.lower()
assert "refs/heads/main" not in release
assert 'git push origin "${RELEASE_SHA}:refs/heads/main"' not in release
assert release.index("Reserve immutable release tag") < release.index("Build and package product assets")
assert release.index("Build and package product assets") < release.index("Publish and verify release assets")
assert release.count('bash "${RUNNER_TEMP}/flux-purr-deploy-edgeone.sh"') == 2
assert "--reservation-id" in release
assert "releaseIntent" in release

for workflow in (ci_pr, ci_main):
    assert "release_chain.py" not in workflow
    assert "prepared" not in workflow
    assert "label" not in workflow.lower()
    assert "test-manual-release.sh" in workflow

for old_path in (
    ".github/workflows/label-gate.yml",
    ".github/workflows/release-preparation.yml",
    ".github/workflows/release-completion.yml",
    ".github/scripts/label-gate.sh",
    ".github/scripts/release_chain.py",
    ".github/scripts/release_preparation.py",
    ".github/scripts/release_completion.py",
    ".github/scripts/release_snapshot.py",
):
    assert not (root / old_path).exists(), old_path

assert quality["required_checks"] == [
    "Firmware checks",
    "DEVD checks",
    "DEVD Windows checks",
    "Web checks",
    "Worktree bootstrap",
]
assert not any("Label" in item["workflow"] for item in quality["expected_pr_workflows"])
assert "expected_preparation_workflows" not in quality
assert all("Release completion" not in item["workflow"] for item in quality["expected_pr_workflows"])
assert quality["expected_release_workflows"][0]["workflow"] == "Release Product"

assert "labels" not in notify.lower()
assert "commit_sha=" not in notify
assert "workflow_run.conclusion == 'failure'" in notify
assert "operation=recover" in notify
assert "reservation" in notify
assert "same-identity" in notify
assert "canonical_tags" in notify
assert "continue-on-error: true" in notify
assert "always()" in notify

assert '"edgeone@${cli_version}" makers deploy' in edgeone_helper
assert 'max_attempts="${EDGEONE_MAX_ATTEMPTS:-3}"' in edgeone_helper
assert "deployment_started" in edgeone_helper
assert "retryable_preflight_failure" in edgeone_helper
assert "PIPESTATUS" in edgeone_helper

print("release workflow fixtures passed")
PY
