#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT
python3 -m py_compile "${root_dir}/scripts/product-version.py" "${root_dir}/.github/scripts/release_chain.py"
git init -q "${tmp_dir}"
git -C "${tmp_dir}" config user.name "Release Chain Test"
git -C "${tmp_dir}" config user.email "release-chain-test@example.invalid"
cp "${root_dir}/scripts/product-version.py" "${tmp_dir}/product-version.py"
cp "${root_dir}/.github/scripts/release_chain.py" "${tmp_dir}/release_chain.py"
cp "${root_dir}/VERSION" "${tmp_dir}/VERSION"
git -C "${tmp_dir}" add .
git -C "${tmp_dir}" commit -q --signoff -m "source"
python3 - "${root_dir}" "${tmp_dir}" <<'PY'
import importlib.util
import subprocess
import sys
from argparse import Namespace
from pathlib import Path

root = Path(sys.argv[1])
tmp = Path(sys.argv[2])
spec = importlib.util.spec_from_file_location("release_chain", root / ".github/scripts/release_chain.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.ROOT = tmp

def run(*args):
    return subprocess.run(args, cwd=tmp, check=True, text=True, capture_output=True).stdout.strip()

source = run("git", "rev-parse", "HEAD")
assert (tmp / "VERSION").read_text(encoding="utf-8") == "0.22.0\n"
module.stage(Namespace(source_sha=source, mode="automatic", exact_version=None, github_output=None))
release = run("git", "rev-parse", "HEAD")
values = module.verify_release_commit(release, source, "0.22.1")
assert values["tag"] == "v0.22.1"
assert module.diff_names(release) == ["VERSION"]
assert module.commit_parent(release) == source

(tmp / "README").write_text("rc source\n", encoding="utf-8")
run("git", "add", "README")
run("git", "commit", "--signoff", "-m", "rc source")
rc_source = run("git", "rev-parse", "HEAD")
module.stage(Namespace(source_sha=rc_source, mode="intent", exact_version=None, release_level="minor", release_channel="rc", github_output=None))
intent_release = run("git", "rev-parse", "HEAD")
assert module.verify_release_commit(intent_release, rc_source, "0.23.0-rc.1")["tag"] == "v0.23.0-rc.1"

run("git", "reset", "--hard", rc_source)
module.stage(Namespace(source_sha=rc_source, mode="exact", exact_version="0.23.0-rc.1", github_output=None))
rc_release = run("git", "rev-parse", "HEAD")
assert module.verify_release_commit(rc_release, rc_source, "0.23.0-rc.1")["version"] == "0.23.0-rc.1"
module.promote(Namespace(commit=rc_release, exact_version=None, github_output=None))
stable_release = run("git", "rev-parse", "HEAD")
assert module.verify_release_commit(stable_release, rc_release, "0.23.0")["version"] == "0.23.0"
print("release chain fixture passed")
PY
