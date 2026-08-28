#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

write_event() {
  local file="$1"
  shift
  {
    printf '{"pull_request":{"labels":['
    local first=1
    for label in "$@"; do
      if [[ "${first}" -eq 0 ]]; then
        printf ','
      fi
      first=0
      jq -nc --arg name "${label}" '{name: $name}'
    done
    printf ']}}\n'
  } > "${file}"
}

expect_pass() {
  local name="$1"
  shift
  local event="${tmp_dir}/${name}.json"
  write_event "${event}" "$@"
  GITHUB_EVENT_NAME=pull_request_target GITHUB_EVENT_PATH="${event}" bash "${root_dir}/.github/scripts/label-gate.sh" >/dev/null
}

expect_fail() {
  local name="$1"
  shift
  local event="${tmp_dir}/${name}.json"
  write_event "${event}" "$@"
  if GITHUB_EVENT_NAME=pull_request_target GITHUB_EVENT_PATH="${event}" bash "${root_dir}/.github/scripts/label-gate.sh" >/dev/null 2>&1; then
    echo "Expected ${name} to fail" >&2
    exit 1
  fi
}

expect_pass valid-release type:patch channel:stable
expect_pass valid-docs-skip type:docs channel:rc
expect_fail missing-type channel:stable
expect_fail duplicate-type type:patch type:minor channel:stable
expect_fail unknown-type type:feature channel:stable
expect_fail missing-channel type:patch
expect_fail duplicate-channel type:patch channel:stable channel:rc
expect_fail unknown-channel type:patch channel:beta

python3 - <<'PY'
import importlib.util
import argparse
import json
import re
import tempfile
from types import SimpleNamespace
from pathlib import Path

path = Path(".github/scripts/release_snapshot.py")
spec = importlib.util.spec_from_file_location("release_snapshot", path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)

assert module.release_fields("type:patch", "channel:stable") == (True, "patch", "pr_labels")
assert module.release_fields("type:minor", "channel:rc") == (True, "minor", "pr_labels")
assert module.release_fields("type:docs", "channel:stable") == (False, "", "skip_type_label")
assert module.release_fields("type:skip", "channel:rc") == (False, "", "skip_type_label")

rc_target = "c" * 40
rc_snapshot = {
    "schema_version": 1,
    "target_sha": rc_target,
    "snapshot_source": "frozen_pr_marker",
    "pr_number": 7,
    "pr_title": "candidate",
    "pr_head_sha": "d" * 40,
    "type_label": "type:minor",
    "channel_label": "channel:rc",
    "release_enabled": True,
    "release_level": "minor",
    "release_channel": "rc",
    "release_reason": "frozen_pr_labels",
    "product": {"effective_version": "0.21.0", "tag": "v0.21.0-rc.ccccccc"},
}
promotion = module.build_promotion_record(rc_target, rc_snapshot)
assert promotion["candidate_snapshot_digest"] == module.snapshot_digest(rc_snapshot)
assert promotion["tag"] == "v0.21.0"
assert module.validate_promotion(promotion, rc_target, rc_snapshot) == promotion
promoted = module.promoted_snapshot(rc_snapshot, promotion)
assert promoted["release_channel"] == "stable"
assert promoted["channel_label"] == "channel:stable"
assert promoted["product"] == {"effective_version": "0.21.0", "tag": "v0.21.0"}
assert rc_snapshot["release_channel"] == "rc"
try:
    module.validate_promotion(dict(promotion, tag="v0.21.1"), rc_target, rc_snapshot)
except module.SnapshotError:
    pass
else:
    raise AssertionError("promotion record with a changed stable tag must fail")
try:
    module.build_promotion_record(rc_target, dict(rc_snapshot, release_channel="stable", channel_label="channel:stable"))
except module.SnapshotError:
    pass
else:
    raise AssertionError("stable snapshots must not be promoted")

originals = {
    "run_git": module.run_git,
    "fetch_notes": module.fetch_notes,
    "read_snapshot": module.read_snapshot,
    "read_promotion": module.read_promotion,
    "stable_tag_commit": module.stable_tag_commit,
    "add_note": module.add_note,
    "push_promotion_with_retry": module.push_promotion_with_retry,
}
try:
    writes = []
    module.run_git = lambda *args, **kwargs: SimpleNamespace(returncode=0, stdout="", stderr="")
    module.fetch_notes = lambda notes_ref: None
    module.read_snapshot = lambda notes_ref, target_sha: rc_snapshot
    module.read_promotion = lambda promotions_ref, target_sha, snapshot: None
    module.stable_tag_commit = lambda tag: None
    module.add_note = lambda notes_ref, target_sha, record: writes.append((notes_ref, target_sha, record))
    module.push_promotion_with_retry = lambda *args, **kwargs: None
    with tempfile.NamedTemporaryFile("w", encoding="utf-8") as output:
        resolved = module.resolve_release(
            "promote",
            rc_target,
            "refs/notes/test-snapshots",
            "refs/notes/test-promotions",
            output.name,
        )
        assert resolved["release_channel"] == "stable"
        assert resolved["product"]["tag"] == "v0.21.0"
        assert writes[0][0] == "refs/notes/test-promotions"
        assert "release_channel=stable" in Path(output.name).read_text(encoding="utf-8")

    module.stable_tag_commit = lambda tag: "e" * 40
    try:
        module.resolve_release(
            "promote",
            rc_target,
            "refs/notes/test-snapshots",
            "refs/notes/test-promotions",
            "",
        )
    except module.SnapshotError:
        pass
    else:
        raise AssertionError("conflicting stable tag must fail before promotion")
finally:
    for name, original in originals.items():
        setattr(module, name, original)

legacy_target = "b" * 40
legacy_snapshot = {
    "schema_version": 1,
    "target_sha": legacy_target,
    "type_label": "type:minor",
    "channel_label": "channel:stable",
    "release_enabled": True,
    "release_level": "minor",
    "release_channel": "stable",
    "release_reason": "frozen_pr_labels",
    "components": {
        "web": {"effective_version": "0.2.8", "tag": "web/v0.2.8"},
        "firmware": {"effective_version": "0.2.5", "tag": "fw/v0.2.5"},
    },
}
assert module.validate_snapshot(legacy_snapshot, legacy_target) == legacy_snapshot
try:
    module.validate_snapshot(
        {key: value for key, value in legacy_snapshot.items() if key != "components"},
        legacy_target,
    )
except module.SnapshotError:
    pass
else:
    raise AssertionError("release snapshot without product or legacy components must fail")

original_run_git = module.run_git
original_read_snapshot = module.read_snapshot
try:
    def fake_pending_run_git(*args, check=True, **kwargs):
        if args == ("rev-list", "--first-parent", "--reverse", legacy_target):
            return SimpleNamespace(stdout="old\n" + legacy_target + "\n")
        raise AssertionError(args)

    module.run_git = fake_pending_run_git
    module.read_snapshot = lambda notes_ref, sha: legacy_snapshot if sha == "old" else None
    assert module.pending_stable_versions("refs/notes/test", legacy_target) == [(0, 2, 8), (0, 2, 5)]
finally:
    module.run_git = original_run_git
    module.read_snapshot = original_read_snapshot

original_git_output = module.git_output
original_run_git = module.run_git
try:
    tag_commits = {
        "v1.0.0": "past",
        "v9.0.0": "future",
    }

    def fake_git_output(*args):
        if args == ("tag", "--list", "v[0-9]*.[0-9]*.[0-9]*"):
            return "\n".join(tag_commits)
        if args[:3] == ("rev-list", "-n", "1"):
            return tag_commits[args[3]]
        raise AssertionError(args)

    def fake_run_git(*args, check=True, **kwargs):
        if args == ("merge-base", "--is-ancestor", "past", "target"):
            return SimpleNamespace(returncode=0)
        if args == ("merge-base", "--is-ancestor", "future", "target"):
            return SimpleNamespace(returncode=1)
        raise AssertionError(args)

    module.git_output = fake_git_output
    module.run_git = fake_run_git
    assert module.max_stable_version("0.1.0", "target", []) == (1, 0, 0)
finally:
    module.git_output = original_git_output
    module.run_git = original_run_git

payload = {
    "schema_version": 1,
    "pr_number": 42,
    "pr_head_sha": "a" * 40,
    "type_label": "type:minor",
    "channel_label": "channel:rc",
}
body = module.intent_comment_body(payload)
assert module.parse_intent_comment(body) == payload
trusted_user = {"login": "github-actions[bot]", "type": "Bot"}
untrusted_user = {"login": "octocat", "type": "User"}

original_pr_comments = module.pr_comments
try:
    module.pr_comments = lambda api_root, token, repository, pr_number: [
        {"body": module.intent_comment_body(dict(payload, type_label="type:major")), "user": untrusted_user},
        {"body": body, "user": trusted_user},
    ]
    assert module.load_frozen_intent("https://example.invalid", "token", "owner/repo", 42, "a" * 40) == {
        "type_label": "type:minor",
        "channel_label": "channel:rc",
    }
finally:
    module.pr_comments = original_pr_comments

original_github_json = module.github_json
try:
    def fake_github_json(api_root, token, repository, path):
        if path.endswith("page=1"):
            return [{"id": idx} for idx in range(100)]
        if path.endswith("page=2"):
            return [{"id": 101}]
        raise AssertionError(path)

    module.github_json = fake_github_json
    assert len(module.pr_comments("https://example.invalid", "token", "owner/repo", 42)) == 101
finally:
    module.github_json = original_github_json

original_pr_comments = module.pr_comments
original_github_request = module.github_request
try:
    calls = []
    old_payload = dict(payload, pr_head_sha="b" * 40)
    module.pr_comments = lambda api_root, token, repository, pr_number: [
        {"id": 6, "body": module.intent_comment_body(payload), "user": untrusted_user},
        {"id": 7, "body": module.intent_comment_body(old_payload), "user": trusted_user},
    ]
    module.github_request = lambda *args, **kwargs: calls.append((args, kwargs))
    module.write_frozen_intent("https://example.invalid", "token", "owner/repo", payload)
    assert calls[0][0][3] == "/issues/42/comments"
    calls.clear()
    module.pr_comments = lambda api_root, token, repository, pr_number: [
        {"id": 8, "body": module.intent_comment_body(payload), "user": trusted_user}
    ]
    module.write_frozen_intent("https://example.invalid", "token", "owner/repo", payload)
    assert calls[0][0][3] == "/issues/comments/8"
finally:
    module.pr_comments = original_pr_comments
    module.github_request = original_github_request

original_write_frozen_intent = module.write_frozen_intent
try:
    writes = []
    module.write_frozen_intent = lambda *args, **kwargs: writes.append((args, kwargs))
    with tempfile.NamedTemporaryFile("w", encoding="utf-8") as event:
        json.dump({"pull_request": {"state": "closed"}}, event)
        event.flush()
        module.cmd_capture_intent(
            argparse.Namespace(
                event_path=event.name,
                api_root="https://example.invalid",
                github_token="token",
                github_repository="owner/repo",
            )
        )
    assert writes == []
finally:
    module.write_frozen_intent = original_write_frozen_intent

original_github_json = module.github_json
original_load_frozen_intent = module.load_frozen_intent
original_parent_has_frozen_intent_gate = module.parent_has_frozen_intent_gate
try:
    def rollout_github_json(api_root, token, repository, path):
        if path == "/commits/" + ("c" * 40) + "/pulls":
            return [{"number": 42, "title": "rollout", "head": {"sha": "a" * 40}}]
        if path == "/issues/42/labels":
            return [{"name": "type:skip"}, {"name": "channel:stable"}]
        raise AssertionError(path)

    module.github_json = rollout_github_json
    module.load_frozen_intent = lambda *args, **kwargs: (_ for _ in ()).throw(module.SnapshotError("missing marker"))
    module.parent_has_frozen_intent_gate = lambda target_sha: False
    snapshot = module.build_snapshot("https://example.invalid", "token", "owner/repo", "c" * 40, "refs/notes/test")
    assert snapshot["snapshot_source"] == "rollout_pr_labels"
    assert snapshot["release_enabled"] is False
finally:
    module.github_json = original_github_json
    module.load_frozen_intent = original_load_frozen_intent
    module.parent_has_frozen_intent_gate = original_parent_has_frozen_intent_gate

manifest_path = Path(".github/scripts/product_release_manifest.py")
manifest_spec = importlib.util.spec_from_file_location("product_release_manifest", manifest_path)
manifest_module = importlib.util.module_from_spec(manifest_spec)
assert manifest_spec.loader is not None
manifest_spec.loader.exec_module(manifest_module)

with tempfile.TemporaryDirectory() as td:
    root = Path(td)
    asset = root / "web.tar.gz"
    asset.write_bytes(b"web")
    component = json.dumps({
        "id": "web",
        "version": "0.2.0",
        "protocolVersions": ["flux-purr.http.v1"],
        "assets": ["web.tar.gz"],
    })
    args = argparse.Namespace(
        version="0.2.0",
        tag="v0.2.0",
        source_sha="c" * 40,
        asset_root=str(root),
        previous_manifest=None,
        component=[component],
        output=str(root / "manifest.json"),
    )
    manifest = manifest_module.build_manifest(args)
    assert manifest["components"][0]["changedSincePrevious"] is True
    previous = root / "previous.json"
    previous.write_text(json.dumps(manifest), encoding="utf-8")
    args.previous_manifest = str(previous)
    unchanged = manifest_module.build_manifest(args)
    assert unchanged["components"][0]["changedSincePrevious"] is False

release_workflow = Path(".github/workflows/release.yml").read_text(encoding="utf-8")
assert "      operation:" in release_workflow
assert "          - recover" in release_workflow
assert "          - promote" in release_workflow
assert "type: choice" in release_workflow
assert "--operation \"${OPERATION}\"" in release_workflow
assert "--promotions-ref \"refs/notes/release-promotions\"" in release_workflow
assert "./.github/actions/setup-linux-serial-deps" in release_workflow
assert "Verify existing release identity" in release_workflow
assert "Existing release manifest asset mismatch" in release_workflow
for workflow_path in (".github/workflows/ci.yml", ".github/workflows/ci-main.yml"):
    workflow = Path(workflow_path).read_text(encoding="utf-8")
    assert "./.github/actions/setup-linux-serial-deps" in workflow

def workflow_job(name):
    start = release_workflow.index(f"  {name}:")
    remainder = release_workflow[start + len(f"  {name}:") :]
    next_job = re.search(r"\n  [a-z][a-z-]+:\n", remainder)
    return remainder if next_job is None else remainder[: next_job.start()]

for job_name in ("firmware", "host-tools"):
    job = workflow_job(job_name)
    assert job.index("Checkout workflow helpers") < job.index("Setup Linux serial build dependencies")
    assert job.index("Setup Linux serial build dependencies") < job.index("Checkout target")
PY

echo "Release label tests passed."
