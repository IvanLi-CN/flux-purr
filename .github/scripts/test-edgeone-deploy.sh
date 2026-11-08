#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
deploy_script="${root_dir}/.github/scripts/deploy-edgeone.sh"
fake_bunx="${root_dir}/.github/scripts/test-fixtures/fake-edgeone-bunx"
test_root="$(mktemp -d)"
trap 'rm -rf "${test_root}"' EXIT

mkdir -p "${test_root}/site"

count_file="${test_root}/retry-count"
EDGEONE_API_TOKEN='<REDACTED>' \
EDGEONE_BUNX_BIN="${fake_bunx}" \
EDGEONE_TEST_COUNT_FILE="${count_file}" \
EDGEONE_TEST_FAIL_UNTIL=2 \
EDGEONE_MAX_ATTEMPTS=3 \
EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
bash "${deploy_script}" "${test_root}/site" makers-project web-production
test "$(<"${count_file}")" = 3

: > "${count_file}"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=2 \
  EDGEONE_MAX_ATTEMPTS=2 \
  EDGEONE_RETRY_DELAYS_SECONDS='0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production; then
  echo "expected bounded EdgeOne deployment retries to fail" >&2
  exit 1
fi
test "$(<"${count_file}")" = 2

echo "EdgeOne deployment retry tests passed"
