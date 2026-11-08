#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 <deploy-directory> <project-name> <target-label>" >&2
}

if [[ $# -ne 3 ]]; then
  usage
  exit 2
fi

deploy_directory="$1"
project_name="$2"
target_label="$3"

: "${EDGEONE_API_TOKEN:?repository secret EDGEONE_API_TOKEN is required}"

if [[ ! -d "${deploy_directory}" ]]; then
  echo "EdgeOne deploy directory does not exist: ${deploy_directory}" >&2
  exit 1
fi

if [[ -z "${project_name}" ]]; then
  echo "EdgeOne project name is required for ${target_label}" >&2
  exit 1
fi

cli_version="${EDGEONE_CLI_VERSION:-1.6.18}"
bunx_bin="${EDGEONE_BUNX_BIN:-bunx}"
max_attempts="${EDGEONE_MAX_ATTEMPTS:-3}"
retry_delays_text="${EDGEONE_RETRY_DELAYS_SECONDS:-30 60}"
attempt_logs_dir="$(mktemp -d)"
trap 'rm -rf -- "${attempt_logs_dir}"' EXIT

if ! [[ "${max_attempts}" =~ ^[1-9][0-9]*$ ]]; then
  echo "EDGEONE_MAX_ATTEMPTS must be a positive integer" >&2
  exit 2
fi

read -r -a retry_delays <<< "${retry_delays_text}"
required_delays=$((max_attempts - 1))
if (( ${#retry_delays[@]} < required_delays )); then
  echo "EDGEONE_RETRY_DELAYS_SECONDS must provide at least ${required_delays} delay values" >&2
  exit 2
fi

for delay in "${retry_delays[@]:0:required_delays}"; do
  if ! [[ "${delay}" =~ ^[0-9]+$ ]]; then
    echo "EDGEONE_RETRY_DELAYS_SECONDS values must be non-negative integers" >&2
    exit 2
  fi
done

run_deploy() {
  local output_file="$1"
  local -a pipeline_status
  local status
  local tee_status

  set +e
  "${bunx_bin}" "edgeone@${cli_version}" makers deploy \
    "${deploy_directory}" \
    -n "${project_name}" \
    -e production \
    -t "${EDGEONE_API_TOKEN}" 2>&1 | tee "${output_file}"
  pipeline_status=("${PIPESTATUS[@]}")
  status="${pipeline_status[0]}"
  tee_status="${pipeline_status[1]}"
  set -e
  attempt_output_ok=1
  if (( tee_status != 0 )); then
    attempt_output_ok=0
  fi
  return "${status}"
}

deployment_started() {
  grep -Eiq \
    'Uploading folder to EdgeOne COS|Creating deployment in .* environment|Created deployment with Deployment ID|Waiting for deployment to complete|Deployment in progress|Deploy Success|Deploy URL:|Console URL:' \
    "$1"
}

retryable_preflight_failure() {
  grep -Eiq \
    'check and set region failed:.*(AxiosError: timeout|AxiosError: Network Error|ETIMEDOUT|ECONNRESET|ECONNREFUSED|ENOTFOUND|socket hang up)' \
    "$1"
}

for ((attempt = 1; attempt <= max_attempts; attempt++)); do
  attempt_log="${attempt_logs_dir}/attempt-${attempt}.log"
  echo "EdgeOne deployment attempt ${attempt}/${max_attempts} for ${target_label}."
  if run_deploy "${attempt_log}"; then
    status=0
  else
    status=$?
  fi

  if (( attempt_output_ok == 0 )); then
    echo "EdgeOne CLI output capture failed; automatic retry is disabled for ${target_label}." >&2
    if (( status == 0 )); then
      exit 1
    fi
    exit "${status}"
  fi

  if (( status == 0 )); then
    echo "EdgeOne deployment succeeded for ${target_label}."
    exit 0
  fi

  if deployment_started "${attempt_log}"; then
    echo "EdgeOne deployment reached an ambiguous remote state; automatic retry is disabled for ${target_label}." >&2
    exit "${status}"
  fi

  if ! retryable_preflight_failure "${attempt_log}"; then
    echo "EdgeOne deployment failed before a retryable endpoint preflight for ${target_label}; automatic retry is disabled." >&2
    if grep -Eiq 'Invalid EDGEONE_PAGES_API_TOKEN|invalid.*token' "${attempt_log}"; then
      echo "The CLI reported a token or configuration error without transport evidence; verify the token scope and project configuration." >&2
    fi
    exit "${status}"
  fi

  if (( attempt == max_attempts )); then
    echo "EdgeOne deployment failed after ${max_attempts} preflight attempts for ${target_label}." >&2
    echo "The CLI reported an endpoint or network failure; inspect preceding output for timeout, reset, refusal, or DNS details." >&2
    exit "${status}"
  fi

  delay="${retry_delays[attempt - 1]}"
  echo "EdgeOne preflight attempt ${attempt} failed with exit ${status}; retrying in ${delay}s." >&2
  sleep "${delay}"
done
