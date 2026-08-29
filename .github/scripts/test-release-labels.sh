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
      if [[ "${first}" -eq 0 ]]; then printf ','; fi
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
from pathlib import Path

path = Path('.github/scripts/release_snapshot.py')
spec = importlib.util.spec_from_file_location('release_snapshot', path)
module = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(module)

assert module.release_fields('type:patch', 'channel:stable') == (True, 'patch', 'pr_labels')
assert module.release_fields('type:docs', 'channel:rc') == (False, '', 'skip_type_label')
assert module.validate_intent_labels([{'name': 'type:minor'}, {'name': 'channel:rc'}]) == ('type:minor', 'channel:rc', [])
try:
    module.validate_intent_labels([{'name': 'type:patch'}, {'name': 'type:minor'}, {'name': 'channel:stable'}])
except module.SnapshotError:
    pass
else:
    raise AssertionError('duplicate type labels must fail')

payload = {
    'schema_version': 2,
    'target_sha': 'a' * 40,
    'merge_commit_sha': 'a' * 40,
    'labels': ['type:patch', 'channel:stable', 'component:web'],
    'pr_number': 7,
    'pr_head_sha': 'b' * 40,
    'type_label': 'type:patch',
    'channel_label': 'channel:stable',
    'components': ['component:web'],
    'release_enabled': True,
    'release_level': 'patch',
    'release_channel': 'stable',
    'release_reason': 'frozen_pr_labels',
}
assert module.validate_snapshot(payload, 'a' * 40) == payload
assert module.release_action(payload) == 'automatic'
payload['type_label'] = 'type:minor'
assert module.release_action(payload) == 'exact'
payload['type_label'] = 'type:patch'
payload['channel_label'] = 'channel:rc'
assert module.release_action(payload) == 'exact'
payload['channel_label'] = 'channel:stable'
assert payload.get('version') is None
legacy = {
    'schema_version': 1,
    'target_sha': 'c' * 40,
    'type_label': 'type:minor',
    'channel_label': 'channel:stable',
    'release_enabled': True,
    'release_level': 'minor',
    'release_channel': 'stable',
    'components': {'web': {'effective_version': '0.1.0', 'tag': 'web/v0.1.0'}},
}
assert module.validate_snapshot(legacy, 'c' * 40)['version_source'].startswith('historical')
PY

echo "Release label tests passed."
