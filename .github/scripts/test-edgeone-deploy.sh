#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
deploy_script="${root_dir}/.github/scripts/deploy-edgeone.sh"
fake_bunx="${root_dir}/.github/scripts/test-fixtures/fake-edgeone-bunx"
test_root="$(mktemp -d)"
trap 'rm -rf "${test_root}"' EXIT

mkdir -p "${test_root}/site"

count_file="${test_root}/retry-count"
args_file="${test_root}/args"
EDGEONE_API_TOKEN='<REDACTED>' \
EDGEONE_BUNX_BIN="${fake_bunx}" \
EDGEONE_TEST_COUNT_FILE="${count_file}" \
EDGEONE_TEST_ARGS_FILE="${args_file}" \
EDGEONE_TEST_FAIL_UNTIL=2 \
EDGEONE_MAX_ATTEMPTS=3 \
EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
bash "${deploy_script}" "${test_root}/site" makers-project web-production
test "$(<"${count_file}")" = 3
first_args="$(head -n1 "${args_file}")"
case "${first_args}" in
  *"edgeone@1.6.18 makers deploy"*"-n makers-project -e production -t <REDACTED>"*) ;;
  *)
    echo "EdgeOne CLI arguments were not forwarded as expected" >&2
    exit 1
    ;;
esac

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

: > "${count_file}"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=permanent \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production; then
  echo "expected permanent EdgeOne failures to stop without retry" >&2
  exit 1
fi
test "$(<"${count_file}")" = 1

: > "${count_file}"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=post-deploy \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production; then
  echo "expected ambiguous post-deploy failures to stop without retry" >&2
  exit 1
fi
test "$(<"${count_file}")" = 1

echo "EdgeOne deployment retry tests passed"
