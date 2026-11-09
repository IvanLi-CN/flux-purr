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
grep -Fq "EDGEONE_FAILURE_CLASS=endpoint_unavailable" "${bounded_failure_log}"
grep -Fq "endpoint or network failure" "${bounded_failure_log}"

: > "${count_file}"
too_many_attempts_log="${test_root}/too-many-attempts.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_MAX_ATTEMPTS=4 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${too_many_attempts_log}" 2>&1; then
  echo "expected an attempt limit above three to be rejected" >&2
  exit 1
else
  too_many_attempts_status=$?
fi
test "${too_many_attempts_status}" = 2
test ! -s "${count_file}"
grep -Fq "integer from 1 to 3" "${too_many_attempts_log}"

: > "${count_file}"
too_many_delay_log="${test_root}/too-many-delay.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_MAX_ATTEMPTS=2 \
  EDGEONE_RETRY_DELAYS_SECONDS='301' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${too_many_delay_log}" 2>&1; then
  echo "expected a retry delay above 300 seconds to be rejected" >&2
  exit 1
else
  too_many_delay_status=$?
fi
test "${too_many_delay_status}" = 2
test ! -s "${count_file}"
grep -Fq "integers from 0 to 300" "${too_many_delay_log}"

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
grep -Fq "EDGEONE_FAILURE_CLASS=auth_or_endpoint_ambiguous" "${permanent_failure_log}"
grep -Fq "token invalidity is unproven" "${permanent_failure_log}"
if grep -Eqi 'token (is )?invalid([.![:space:]]|$)' "${permanent_failure_log}"; then
  echo "ambiguous EdgeOne output must not claim token invalidity" >&2
  exit 1
fi

: > "${count_file}"
ambiguous_endpoints_evidence="${test_root}/ambiguous-endpoints-evidence.log"
ambiguous_endpoints_log="${test_root}/ambiguous-endpoints.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_INTERNAL_EVIDENCE_FILE="${ambiguous_endpoints_evidence}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=ambiguous-endpoints \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${ambiguous_endpoints_log}" 2>&1; then
  echo "expected ambiguous two-endpoint failures to stop without retry" >&2
  exit 1
else
  ambiguous_endpoints_status=$?
fi
test "$(<"${count_file}")" = 1
test "${ambiguous_endpoints_status}" = 7
test "$(<"${ambiguous_endpoints_evidence}")" = $'endpoint-1: 522\nendpoint-2: 522'
grep -Fq "EDGEONE_FAILURE_CLASS=auth_or_endpoint_ambiguous" "${ambiguous_endpoints_log}"

: > "${count_file}"
auth_rejected_log="${test_root}/auth-rejected.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=auth-rejected \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${auth_rejected_log}" 2>&1; then
  echo "expected explicit EdgeOne auth rejection to fail" >&2
  exit 1
else
  auth_rejected_status=$?
fi
test "$(<"${count_file}")" = 1
test "${auth_rejected_status}" = 7
grep -Fq "EDGEONE_FAILURE_CLASS=credential_rejected" "${auth_rejected_log}"

: > "${count_file}"
project_permission_log="${test_root}/project-permission.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=project-permission \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${project_permission_log}" 2>&1; then
  echo "expected explicit EdgeOne project permission failure" >&2
  exit 1
else
  project_permission_status=$?
fi
test "$(<"${count_file}")" = 1
test "${project_permission_status}" = 7
grep -Fq "EDGEONE_FAILURE_CLASS=project_or_permission" "${project_permission_log}"

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
remote_capture_failure_log="${test_root}/remote-capture-failure.log"
if PATH="${root_dir}/.github/scripts/test-fixtures:${PATH}" \
  EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=post-deploy \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${remote_capture_failure_log}" 2>&1; then
  echo "expected remote output capture failure to stop without retry" >&2
  exit 1
else
  remote_capture_failure_status=$?
fi
test "$(<"${count_file}")" = 1
test "${remote_capture_failure_status}" = 7
grep -Fq "EDGEONE_FAILURE_CLASS=deployment_ambiguous" "${remote_capture_failure_log}"
grep -Fq "ambiguous remote state" "${remote_capture_failure_log}"

: > "${count_file}"
successful_capture_failure_log="${test_root}/successful-capture-failure.log"
if PATH="${root_dir}/.github/scripts/test-fixtures:${PATH}" \
  EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${successful_capture_failure_log}" 2>&1; then
  successful_capture_status=0
else
  successful_capture_status=$?
fi
test "$(<"${count_file}")" = 1
test "${successful_capture_status}" = 0
grep -Fq "treating CLI success as authoritative" "${successful_capture_failure_log}"

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
grep -Fq "EDGEONE_FAILURE_CLASS=deployment_ambiguous" "${unknown_postdeploy_failure_log}"
grep -Fq "ambiguous remote state" "${unknown_postdeploy_failure_log}"

: > "${count_file}"
mixed_preflight_failure_log="${test_root}/mixed-preflight-failure.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=mixed-preflight \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${mixed_preflight_failure_log}" 2>&1; then
  echo "expected mixed preflight output to stop without retry" >&2
  exit 1
else
  mixed_preflight_status=$?
fi
test "$(<"${count_file}")" = 1
test "${mixed_preflight_status}" = 7
grep -Fq "before a retryable endpoint preflight" "${mixed_preflight_failure_log}"

: > "${count_file}"
same_line_mixed_preflight_failure_log="${test_root}/same-line-mixed-preflight-failure.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=same-line-mixed-preflight \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${same_line_mixed_preflight_failure_log}" 2>&1; then
  echo "expected same-line mixed preflight output to stop without retry" >&2
  exit 1
else
  same_line_mixed_preflight_status=$?
fi
test "$(<"${count_file}")" = 1
test "${same_line_mixed_preflight_status}" = 7
grep -Fq "before a retryable endpoint preflight" "${same_line_mixed_preflight_failure_log}"

: > "${count_file}"
partial_preflight_failure_log="${test_root}/partial-preflight-failure.log"
if EDGEONE_API_TOKEN='<REDACTED>' \
  EDGEONE_BUNX_BIN="${fake_bunx}" \
  EDGEONE_TEST_COUNT_FILE="${count_file}" \
  EDGEONE_TEST_FAIL_UNTIL=1 \
  EDGEONE_TEST_FAILURE_STAGE=partial-preflight \
  EDGEONE_MAX_ATTEMPTS=3 \
  EDGEONE_RETRY_DELAYS_SECONDS='0 0' \
  bash "${deploy_script}" "${test_root}/site" makers-project web-production >"${partial_preflight_failure_log}" 2>&1; then
  echo "expected partial preflight output to stop without retry" >&2
  exit 1
else
  partial_preflight_status=$?
fi
test "$(<"${count_file}")" = 1
test "${partial_preflight_status}" = 7
grep -Fq "before a retryable endpoint preflight" "${partial_preflight_failure_log}"

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
grep -Fq "EDGEONE_FAILURE_CLASS=deployment_ambiguous" "${postdeploy_failure_log}"
grep -Fq "ambiguous remote state" "${postdeploy_failure_log}"

echo "EdgeOne deployment retry tests passed"
