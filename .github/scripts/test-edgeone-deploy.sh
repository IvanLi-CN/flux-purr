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
  *"edgeone@1.6.18 makers deploy ${test_root}/site -n makers-project -e production -t <REDACTED>"*) ;;
  *)
    echo "EdgeOne CLI arguments were not forwarded as expected" >&2
    exit 1
    ;;
esac

: > "${count_file}"
bounded_failure_log="${test_root}/bounded-failure.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=2 \
  EDGEONE_MAX_ATTEMPTS=2 \
  EDGEONE_RETRY_DELAYS_SECONDS='0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${bounded_failure_log}" 2>&1; then
  echo "expected bounded EdgeOne deployment retries to fail" >&2
  exit 1
else
  bounded_status=$?
fi
test "$(<"${count_file}")" = 2
test "${bounded_status}" = 7
grep -Fq "endpoint or network failure" "${bounded_failure_log}"

: > "${count_file}"
permanent_failure_log="${test_root}/permanent-failure.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=permanent \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${permanent_failure_log}" 2>&1; then
  echo "expected permanent EdgeOne failures to stop without retry" >&2
  exit 1
else
  permanent_status=$?
fi
test "$(<"${count_file}")" = 1
test "${permanent_status}" = 7
grep -Fq "token or configuration error without transport evidence" "${permanent_failure_log}"

: > "${count_file}"
capture_failure_log="${test_root}/capture-failure.log"
if PATH="${root_dir}/.github/scripts/test-fixtures:${PATH}" \
  EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=permanent \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${capture_failure_log}" 2>&1; then
  echo "expected output capture failure to stop without retry" >&2
  exit 1
else
  capture_failure_status=$?
fi
test "$(<"${count_file}")" = 1
test "${capture_failure_status}" = 7
grep -Fq "output capture failed" "${capture_failure_log}"

: > "${count_file}"
successful_capture_failure_log="${test_root}/successful-capture-failure.log"
if PATH="${root_dir}/.github/scripts/test-fixtures:${PATH}" \
  EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${successful_capture_failure_log}" 2>&1; then
  echo "expected successful CLI with output capture failure to stop" >&2
  exit 1
else
  successful_capture_status=$?
fi
test "$(<"${count_file}")" = 1
test "${successful_capture_status}" = 1
grep -Fq "output capture failed" "${successful_capture_failure_log}"

: > "${count_file}"
unknown_postdeploy_failure_log="${test_root}/unknown-postdeploy-failure.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=unknown-post-deploy \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${unknown_postdeploy_failure_log}" 2>&1; then
  echo "expected unknown post-deploy failures to stop without retry" >&2
  exit 1
else
  unknown_postdeploy_status=$?
fi
test "$(<"${count_file}")" = 1
test "${unknown_postdeploy_status}" = 7
grep -Fq "before a retryable endpoint preflight" "${unknown_postdeploy_failure_log}"

: > "${count_file}"
postdeploy_failure_log="${test_root}/postdeploy-failure.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=post-deploy \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${postdeploy_failure_log}" 2>&1; then
  echo "expected ambiguous post-deploy failures to stop without retry" >&2
  exit 1
else
  postdeploy_status=$?
fi
test "$(<"${count_file}")" = 1
test "${postdeploy_status}" = 7
grep -Fq "ambiguous remote state" "${postdeploy_failure_log}"

echo "EdgeOne deployment retry tests passed"
