#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

run_compute() {
  local level="$1"
  local channel="$2"
  local env_file="${tmp_dir}/product-${level}-${channel}.env"
  GITHUB_ENV="${env_file}" RELEASE_LEVEL="${level}" RELEASE_CHANNEL="${channel}" \
    bash "${root_dir}/.github/scripts/compute-version-product.sh" >/dev/null
  cat "${env_file}"
}

product_stable="$(run_compute patch stable)"
product_rc="$(run_compute minor rc)"

grep -Eq '^PRODUCT_TAG=v[0-9]+\.[0-9]+\.[0-9]+$' <<<"${product_stable}"
grep -Eq '^PRODUCT_TAG=v[0-9]+\.[0-9]+\.[0-9]+-rc\.[0-9a-f]{7}$' <<<"${product_rc}"

echo "Version script tests passed."
