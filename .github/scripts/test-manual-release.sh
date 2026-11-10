#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

cat > "${tmp_dir}/tags.txt" <<'EOF'
v0.30.7 0000000000000000000000000000000000000001
v0.30.8 0000000000000000000000000000000000000003
v0.30.0-rc.1 0000000000000000000000000000000000000002
EOF

resolve() {
  python3 "${root_dir}/.github/scripts/manual_release.py" resolve --intent "$1" --baseline 0.30.7 --tags-file "${tmp_dir}/tags.txt" \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)["version"])'
}

test "$(resolve patch)" = 0.30.8
test "$(resolve minor)" = 0.31.0
test "$(resolve major)" = 1.0.0
test "$(resolve rc)" = 0.30.8-rc.1

printf '%s\n' docs/spec.md README README.md CHANGELOG.txt docs/image.png > "${tmp_dir}/docs.paths"
printf '%s\n' docs/spec.md web/src/app.tsx > "${tmp_dir}/mixed.paths"
test "$(python3 "${root_dir}/.github/scripts/manual_release.py" classify --paths-file "${tmp_dir}/docs.paths" | python3 -c 'import json,sys; print(json.load(sys.stdin)["release_required"])')" = False
test "$(python3 "${root_dir}/.github/scripts/manual_release.py" classify --paths-file "${tmp_dir}/mixed.paths" | python3 -c 'import json,sys; print(json.load(sys.stdin)["release_required"])')" = True

set +e
python3 "${root_dir}/.github/scripts/manual_release.py" identity \
  --intent patch --baseline 0.30.7 --source-sha 0000000000000000000000000000000000000004 \
  --tags-file "${tmp_dir}/tags.txt" > "${tmp_dir}/collision.out" 2>&1
status=$?
set -e
test "${status}" -ne 0
grep -F "already owned" "${tmp_dir}/collision.out"

set +e
python3 "${root_dir}/.github/scripts/manual_release.py" identity \
  --intent minor --baseline 0.30.7 --source-sha 0000000000000000000000000000000000000001 \
  --tags-file "${tmp_dir}/tags.txt" > "${tmp_dir}/source-tag.out" 2>&1
status=$?
set -e
test "${status}" -ne 0
grep -F "already has release tag" "${tmp_dir}/source-tag.out"

python3 "${root_dir}/.github/scripts/manual_release.py" identity \
  --intent rc --baseline 0.30.7 --source-sha 0000000000000000000000000000000000000002 \
  --tags-file "${tmp_dir}/tags.txt" --operation recover \
  | python3 -c 'import json,sys; value=json.load(sys.stdin); assert value["version"] == "0.30.0-rc.1"; assert value["reservation_status"] == "matching"'

python3 - "${root_dir}" <<'PY'
import argparse
import importlib.util
import tempfile
from pathlib import Path
import sys

root = Path(sys.argv[1])
spec = importlib.util.spec_from_file_location("product_release_manifest", root / ".github/scripts/product_release_manifest.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
with tempfile.TemporaryDirectory() as directory:
    repo = Path(directory)
    (repo / "VERSION").write_text("0.30.8\n", encoding="utf-8")
    assets = repo / "release-assets"
    assets.mkdir()
    (assets / "web.tar.gz").write_bytes(b"web")
    args = argparse.Namespace(
        repo_root=repo,
        version="0.30.8",
        tag="v0.30.8",
        source_sha="0" * 40,
        channel="stable",
        intent="patch",
        baseline="0.30.7",
        reservation_id="v0.30.8@" + "0" * 40,
        previous_manifest=None,
        component=['{"id":"web","version":"0.30.8","channel":"stable","assets":["web.tar.gz"]}'],
        asset_root=str(assets),
    )
    manifest = module.build_manifest(args)
    assert manifest["releaseIntent"] == "patch"
    assert manifest["trustedBaseline"] == "0.30.7"
    assert manifest["reservationId"] == "v0.30.8@" + "0" * 40
PY

echo "manual release policy tests passed"
