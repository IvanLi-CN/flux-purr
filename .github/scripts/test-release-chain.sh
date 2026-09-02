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
printf '0.22.0\n' > "${tmp_dir}/VERSION"
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

# A candidate tag owned by another commit must be rejected before VERSION is
# written or a preparation commit is created.
run("git", "tag", "v0.22.1", source)
before_head = run("git", "rev-parse", "HEAD")
before_count = int(run("git", "rev-list", "--count", "HEAD"))
try:
    module.stage(Namespace(
        source_sha=source,
        mode="automatic",
        exact_version=None,
        expected_channel="stable",
        intent_type="type:patch",
        intent_channel="stable",
        intent_components="none",
        github_output=None,
    ))
except module.ReleaseChainError:
    pass
else:
    raise AssertionError("occupied product tag must block preparation")
assert run("git", "rev-parse", "HEAD") == before_head
assert int(run("git", "rev-list", "--count", "HEAD")) == before_count
assert (tmp / "VERSION").read_text(encoding="utf-8") == "0.22.0\n"
run("git", "tag", "-d", "v0.22.1")

module.stage(Namespace(
    source_sha=source,
    mode="automatic",
    exact_version=None,
    expected_channel="stable",
    intent_type="type:patch",
    intent_channel="stable",
    intent_components="none",
    github_output=None,
))
release = run("git", "rev-parse", "HEAD")
values = module.verify_release_commit(release, source, "0.22.1")
assert values["tag"] == "v0.22.1"
run("git", "tag", "v0.22.1", release)
assert module.verify_tag(values["version"], release, allow_existing=True)["status"] == "matching"
assert module.verify_prepared_commit(release, source, "0.22.1")["action"] == "automatic"
assert module.diff_names(release) == ["VERSION"]
assert module.commit_parent(release) == source
run("git", "tag", "-d", "v0.22.1")

# A normal PR merge can have a tree identical to its second parent.  It must
# not become a product release unless that parent is a verified preparation.
run("git", "branch", "prepared-release", release)
run("git", "checkout", "-q", "-b", "release-main", source)
run("git", "merge", "--no-ff", "--no-edit", "prepared-release")
merged_release = run("git", "rev-parse", "HEAD")
merged_values = module.verify_merged_prepared_release(merged_release)
assert merged_values["prepared"] == "true"
assert merged_values["mergeSha"] == merged_release
assert merged_values["preparationSha"] == release
assert merged_values["version"] == "0.22.1"

# A prepared commit is based on the PR source, which may be ahead of the
# merged main parent. The merge verifier must validate that ancestry instead
# of requiring the preparation parent to equal the main parent.
run("git", "checkout", "-q", "-b", "divergent-source", source)
(tmp / "README").write_text("source change\n", encoding="utf-8")
run("git", "add", "README")
run("git", "commit", "--signoff", "-m", "source change")
divergent_source = run("git", "rev-parse", "HEAD")
module.stage(Namespace(
    source_sha=divergent_source,
    mode="automatic",
    exact_version=None,
    expected_channel="stable",
    intent_type="type:patch",
    intent_channel="stable",
    intent_components="none",
    github_output=None,
))
divergent_preparation = run("git", "rev-parse", "HEAD")
run("git", "checkout", "-q", "-b", "divergent-main", source)
run("git", "merge", "--no-ff", "--no-edit", divergent_preparation)
divergent_merge = run("git", "rev-parse", "HEAD")
divergent_values = module.verify_merged_prepared_release(divergent_merge)
assert divergent_values["prepared"] == "true"
assert divergent_values["sourceSha"] == divergent_source

run("git", "checkout", "-q", "-b", "bootstrap-source", merged_release)
(tmp / "README").write_text("bootstrap workflow change\n", encoding="utf-8")
run("git", "add", "README")
run("git", "commit", "--signoff", "-m", "bootstrap workflow change")
run("git", "checkout", "-q", "release-main")
run("git", "merge", "--no-ff", "--no-edit", "bootstrap-source")
bootstrap_merge = run("git", "rev-parse", "HEAD")
assert module.verify_merged_prepared_release(bootstrap_merge) == {
    "prepared": "false",
    "reason": "no_prepared_product_merge",
}
run("git", "checkout", "-q", "prepared-release")

(tmp / "README").write_text("rc source\n", encoding="utf-8")
run("git", "add", "README")
run("git", "commit", "--signoff", "-m", "rc source")
rc_source = run("git", "rev-parse", "HEAD")
module.stage(Namespace(
    source_sha=rc_source,
    mode="exact",
    exact_version="0.23.0-rc.1",
    expected_channel="rc",
    intent_type="type:patch",
    intent_channel="rc",
    intent_components="none",
    github_output=None,
))
rc_release = run("git", "rev-parse", "HEAD")
assert module.verify_release_commit(rc_release, rc_source, "0.23.0-rc.1")["version"] == "0.23.0-rc.1"
assert module.verify_prepared_commit(rc_release, rc_source, "0.23.0-rc.1")["action"] == "exact"
module.promote(Namespace(commit=rc_release, exact_version=None, github_output=None))
stable_release = run("git", "rev-parse", "HEAD")
assert module.verify_release_commit(stable_release, rc_release, "0.23.0")["version"] == "0.23.0"

run("git", "reset", "--hard", rc_source)
try:
    module.stage(Namespace(source_sha=rc_source, mode="intent", exact_version=None, expected_channel=None, intent_type=None, intent_channel=None, intent_components="none", github_output=None))
except module.ReleaseChainError:
    pass
else:
    raise AssertionError("labels must not select a numeric staging mode")
try:
    module.stage(Namespace(source_sha=rc_source, mode="exact", exact_version="0.23.0", expected_channel="rc", intent_type="type:patch", intent_channel="rc", intent_components="none", github_output=None))
except module.ReleaseChainError:
    pass
else:
    raise AssertionError("exact VERSION must match the frozen release channel")
try:
    module.stage(Namespace(source_sha=rc_source, mode="exact", exact_version="0.22.1", expected_channel="stable", intent_type="type:patch", intent_channel="stable", intent_components="none", github_output=None))
except module.ReleaseChainError:
    pass
else:
    raise AssertionError("exact VERSION must be strictly newer than the source VERSION")
print("release chain fixture passed")
PY
