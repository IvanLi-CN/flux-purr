#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT
git init -q "${tmp_dir}"
git -C "${tmp_dir}" config user.name "Release Completion Test"
git -C "${tmp_dir}" config user.email "release-completion-test@example.invalid"
printf '%s\n' 'legacy' > "${tmp_dir}/README"
git -C "${tmp_dir}" add README
git -C "${tmp_dir}" commit -q --signoff -m legacy
legacy_base="$(git -C "${tmp_dir}" rev-parse HEAD)"
printf '%s\n' '0.22.0' > "${tmp_dir}/VERSION"
git -C "${tmp_dir}" add VERSION
git -C "${tmp_dir}" commit -q --signoff -m migration
migration_head="$(git -C "${tmp_dir}" rev-parse HEAD)"

python3 - "${root_dir}" "${tmp_dir}" "${legacy_base}" "${migration_head}" <<'PY'
import importlib.util
import subprocess
import sys
from pathlib import Path

root = Path(sys.argv[1])
tmp = Path(sys.argv[2])
legacy_base = sys.argv[3]
migration_head = sys.argv[4]
spec = importlib.util.spec_from_file_location("release_completion", root / ".github/scripts/release_completion.py")
assert spec and spec.loader
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
module.ROOT = tmp
module.CHAIN.ROOT = tmp
module.SNAPSHOT.ROOT = tmp

def run(*args: str) -> str:
    return subprocess.run(args, cwd=tmp, check=True, text=True, capture_output=True).stdout.strip()

assert module.main(["--commit", migration_head, "--base", legacy_base, "--allow-migration", "--skip-remote"]) == 0
Path(tmp / "README").write_text("source\n", encoding="utf-8")
run("git", "add", "README")
run("git", "commit", "--signoff", "-m", "source")
source = run("git", "rev-parse", "HEAD")
Path(tmp / "VERSION").write_text("0.22.1\n", encoding="utf-8")
run("git", "add", "VERSION")
run("git", "commit", "--signoff", "-m", "chore(release): v0.22.1", "-m", f"Release-Source-SHA: {source}\nProduct-Version: 0.22.1")
release = run("git", "rev-parse", "HEAD")
run("git", "tag", "-a", "v0.22.1", release, "-m", "v0.22.1")
Path(tmp / "README").write_text("feature\n", encoding="utf-8")
run("git", "add", "README")
run("git", "commit", "--signoff", "-m", "feature")
feature = run("git", "rev-parse", "HEAD")
assert module.main(["--commit", feature, "--base", release, "--skip-remote"]) == 0
skip_payload = {
    "schema_version": 2,
    "target_sha": feature,
    "merge_commit_sha": feature,
    "labels": ["type:skip", "channel:stable"],
    "pr_number": 9,
    "pr_head_sha": "a" * 40,
    "type_label": "type:skip",
    "channel_label": "channel:stable",
    "components": [],
    "release_enabled": False,
    "release_level": "",
    "release_channel": "stable",
    "release_reason": "frozen_skip_type_label",
}
module.SNAPSHOT.add_note(module.SNAPSHOT.DEFAULT_NOTES_REF, feature, skip_payload)
assert module.has_non_release_snapshot(feature)
Path(tmp / "VERSION").write_text("0.22.2\n", encoding="utf-8")
run("git", "add", "VERSION")
run("git", "commit", "--signoff", "-m", "invalid")
invalid = run("git", "rev-parse", "HEAD")
assert module.main(["--commit", invalid, "--base", release, "--skip-remote"]) == 1
print("release completion fixtures passed")
PY
