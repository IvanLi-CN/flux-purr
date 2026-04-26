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

original_git_output = module.git_output
original_run_git = module.run_git
try:
    tag_commits = {
        "web/v1.0.0": "past",
        "web/v9.0.0": "future",
    }

    def fake_git_output(*args):
        if args == ("tag", "--list", "web/v[0-9]*.[0-9]*.[0-9]*"):
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
    assert module.max_stable_version("web", "0.1.0", "target", []) == (1, 0, 0)
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
PY

echo "Release label tests passed."
