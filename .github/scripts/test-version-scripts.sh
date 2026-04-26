#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

run_compute() {
  local component="$1"
  local level="$2"
  local channel="$3"
  local env_file="${tmp_dir}/${component}-${level}-${channel}.env"
  GITHUB_ENV="${env_file}" RELEASE_LEVEL="${level}" RELEASE_CHANNEL="${channel}" \
    bash "${root_dir}/.github/scripts/compute-version-${component}.sh" >/dev/null
  cat "${env_file}"
}

web_stable="$(run_compute web patch stable)"
fw_stable="$(run_compute fw patch stable)"
web_rc="$(run_compute web minor rc)"
fw_rc="$(run_compute fw major rc)"

grep -Eq '^WEB_TAG=web/v[0-9]+\.[0-9]+\.[0-9]+$' <<<"${web_stable}"
grep -Eq '^FW_TAG=fw/v[0-9]+\.[0-9]+\.[0-9]+$' <<<"${fw_stable}"
grep -Eq '^WEB_TAG=web/v[0-9]+\.[0-9]+\.[0-9]+-rc\.[0-9a-f]{7}$' <<<"${web_rc}"
grep -Eq '^FW_TAG=fw/v[0-9]+\.[0-9]+\.[0-9]+-rc\.[0-9a-f]{7}$' <<<"${fw_rc}"

echo "Version script tests passed."
