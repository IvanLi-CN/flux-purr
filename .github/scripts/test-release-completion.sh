#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT
git init -q "${tmp_dir}"
git -C "${tmp_dir}" config user.name "Release Completion Test"
git -C "${tmp_dir}" config user.email "release-completion-test@example.invalid"
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
import subprocess
import sys
import tempfile
from argparse import Namespace
from pathlib import Path

root = Path(sys.argv[1])
tmp = Path(sys.argv[2])
base, source = sys.argv[3:]

def load(name: str, path: Path):
    spec = importlib.util.spec_from_file_location(name, path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

chain = load("release_chain", root / ".github/scripts/release_chain.py")
completion = load("release_completion", root / ".github/scripts/release_completion.py")
chain.ROOT = tmp
completion.ROOT = tmp
completion.CHAIN.ROOT = tmp

def run(*args: str) -> str:
    return subprocess.run(args, cwd=tmp, check=True, text=True, capture_output=True).stdout.strip()

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

chain.stage(Namespace(
    source_sha=source,
    mode="automatic",
    exact_version=None,
    expected_channel="stable",
    intent_type="type:patch",
    intent_channel="stable",
    intent_components="none",
    github_output=None,
))
prepared = run("git", "rev-parse", "HEAD")
assert completion.main([
    "--commit", prepared,
    "--base", base,
    "--labels-json", str(labels),
    "--checks-json", str(checks),
]) == 0

checks.write_text(json.dumps({"check_runs": []}), encoding="utf-8")
assert completion.main([
    "--commit", prepared,
    "--base", base,
    "--labels-json", str(labels),
    "--checks-json", str(checks),
]) == 1
checks.write_text(json.dumps({"check_runs": [
    {"name": "Validate PR labels", "conclusion": "success"},
    {"name": "Firmware checks", "conclusion": "success"},
    {"name": "DEVD checks", "conclusion": "success"},
    {"name": "Web checks", "conclusion": "success"},
    {"name": "Worktree bootstrap", "conclusion": "success"},
]}), encoding="utf-8")

run("git", "reset", "--hard", source)
assert completion.main([
    "--commit", source,
    "--base", base,
    "--labels-json", str(labels),
    "--checks-json", str(checks),
]) == 1

docs_labels = Path(tempfile.mkstemp()[1])
docs_labels.write_text(json.dumps([{"name": "type:skip"}, {"name": "channel:stable"}]), encoding="utf-8")
assert completion.main([
    "--commit", source,
    "--base", base,
    "--labels-json", str(docs_labels),
    "--checks-json", str(checks),
]) == 0

Path(tmp / "VERSION").write_text("0.22.1\n", encoding="utf-8")
run("git", "add", "VERSION")
run("git", "commit", "--signoff", "-m", "invalid version")
invalid = run("git", "rev-parse", "HEAD")
assert completion.main([
    "--commit", invalid,
    "--base", base,
    "--labels-json", str(labels),
    "--checks-json", str(checks),
]) == 1
print("release completion fixtures passed")
PY
