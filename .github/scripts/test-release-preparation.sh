#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT
git init -q "${tmp_dir}"
git -C "${tmp_dir}" config user.name "Release Preparation Test"
git -C "${tmp_dir}" config user.email "release-preparation-test@example.invalid"
printf '%s\n' '0.22.0' > "${tmp_dir}/VERSION"
printf '%s\n' 'base' > "${tmp_dir}/README"
git -C "${tmp_dir}" add VERSION README
git -C "${tmp_dir}" commit -q --signoff -m base
base="$(git -C "${tmp_dir}" rev-parse HEAD)"
printf '%s\n' 'source' >> "${tmp_dir}/README"
git -C "${tmp_dir}" add README
git -C "${tmp_dir}" commit -q --signoff -m source
source="$(git -C "${tmp_dir}" rev-parse HEAD)"

python3 - "${root_dir}" "${tmp_dir}" "${base}" "${source}" <<'PY'
import importlib.util
import json
import sys
import tempfile
from pathlib import Path

root = Path(sys.argv[1])
repo = Path(sys.argv[2])
base, source = sys.argv[3:]
spec = importlib.util.spec_from_file_location("release_preparation", root / ".github/scripts/release_preparation.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)

labels = Path(tempfile.mkstemp()[1])
labels.write_text(json.dumps([{"name": "type:patch"}, {"name": "channel:stable"}]), encoding="utf-8")
checks = Path(tempfile.mkstemp()[1])
checks.write_text(json.dumps({"check_runs": [
    {"name": "Validate PR labels", "conclusion": "success"},
    {"name": "Firmware checks", "conclusion": "success"},
    {"name": "DEVD checks", "conclusion": "success"},
    {"name": "Web checks", "conclusion": "success"},
    {"name": "Worktree bootstrap", "conclusion": "success"},
]}), encoding="utf-8")

assert module.main([
    "--repo-root", str(repo),
    "--source-sha", source,
    "--base-sha", base,
    "--labels-json", str(labels),
    "--checks-json", str(checks),
    "--mode", "automatic",
]) == 0
prepared = module.git(repo, "rev-parse", "HEAD")
module.CHAIN.ROOT = repo
identity = module.CHAIN.verify_prepared_commit(prepared, source, "0.22.1")
assert identity["typeLabel"] == "type:patch"
assert identity["channel"] == "stable"
assert identity["action"] == "automatic"
assert (repo / "VERSION").read_text(encoding="utf-8") == "0.22.1\n"

# A rerun validates the original source checks, does not create another commit,
# and refuses a prepared intent that no longer matches the current labels.
assert module.main([
    "--repo-root", str(repo),
    "--source-sha", prepared,
    "--base-sha", base,
    "--labels-json", str(labels),
    "--checks-json", str(checks),
    "--mode", "automatic",
]) == 0
labels.write_text(json.dumps([
    {"name": "type:patch"},
    {"name": "channel:stable"},
    {"name": "component:firmware"},
]), encoding="utf-8")
assert module.main([
    "--repo-root", str(repo),
    "--source-sha", prepared,
    "--base-sha", base,
    "--labels-json", str(labels),
    "--checks-json", str(checks),
    "--mode", "automatic",
]) == 1
print("release preparation fixtures passed")
PY
