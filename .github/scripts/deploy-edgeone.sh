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

for ((attempt = 1; attempt <= max_attempts; attempt++)); do
  echo "EdgeOne deployment attempt ${attempt}/${max_attempts} for ${target_label}."
  if "${bunx_bin}" "edgeone@${cli_version}" makers deploy \
    "${deploy_directory}" \
    -n "${project_name}" \
    -e production \
    -t "${EDGEONE_API_TOKEN}"; then
    echo "EdgeOne deployment succeeded for ${target_label}."
    exit 0
  else
    status=$?
  fi

  if (( attempt == max_attempts )); then
    echo "EdgeOne deployment failed after ${max_attempts} attempts for ${target_label}." >&2
    echo "The CLI token error is non-diagnostic; inspect preceding output for endpoint, timeout, or authentication details." >&2
    exit "${status}"
  fi

  delay="${retry_delays[attempt - 1]}"
  echo "EdgeOne deployment attempt ${attempt} failed with exit ${status}; retrying in ${delay}s." >&2
  sleep "${delay}"
done
