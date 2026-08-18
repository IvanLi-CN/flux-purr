#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
artifact_root="${FLUX_PURR_FIRMWARE_ARTIFACTS_DIR:-${repo_root}/firmware/target/flux-purr-web-artifacts}"
source_sha="${FLUX_PURR_SOURCE_SHA:-$(git -C "${repo_root}" rev-parse HEAD)}"
build_id="${FLUX_PURR_BUILD_ID:-${source_sha:0:16}}"
if [[ -n "${FLUX_PURR_FIRMWARE_VERSION:-}" ]]; then
  version="${FLUX_PURR_FIRMWARE_VERSION}"
else
  base_version="$(
    git -C "${repo_root}" tag --merged HEAD --sort=-v:refname \
      | sed -nE 's/^v([0-9]+\.[0-9]+\.[0-9]+)$/\1/p' \
      | head -n 1
  )"
  if [[ -z "${base_version}" ]]; then
    base_version="0.1.0"
  fi
  IFS='.' read -r version_major version_minor version_patch <<<"${base_version}"
  version="${version_major}.${version_minor}.$((version_patch + 1))-dev.${build_id:0:7}"
fi
channel="${RELEASE_CHANNEL:-local}"
elf="${repo_root}/firmware/target/xtensa-esp32s3-none-elf/release/flux-purr"
output="${artifact_root}/flux-purr-firmware-${version}-${build_id}.fluxpurr-fw"

if [[ ! "${source_sha}" =~ ^[0-9a-f]{40}$ ]]; then
  echo "FLUX_PURR_SOURCE_SHA must be a 40-character lowercase commit SHA" >&2
  exit 1
fi
if [[ ! "${build_id}" =~ ^[0-9a-f]{16,64}$ ]]; then
  echo "FLUX_PURR_BUILD_ID must be 16-64 lowercase hexadecimal characters" >&2
  exit 1
fi
if [[ "${channel}" != stable && "${channel}" != rc && "${channel}" != local ]]; then
  echo "RELEASE_CHANNEL must be stable, rc, or local" >&2
  exit 1
fi

mkdir -p "${artifact_root}"

FLUX_PURR_FIRMWARE_VERSION="${version}" \
  FLUX_PURR_SOURCE_SHA="${source_sha}" \
  FLUX_PURR_BUILD_ID="${build_id}" \
  bash "${repo_root}/scripts/check-firmware-build.sh"

cargo build \
  --manifest-path "${repo_root}/tools/flux-purr-devd/Cargo.toml" \
  --locked \
  --bin flux-purr-build-firmware-bundle

"${repo_root}/target/debug/flux-purr-build-firmware-bundle" \
  --elf "${elf}" \
  --partition-table "${repo_root}/firmware/partitions.csv" \
  --version "${version}" \
  --source-sha "${source_sha}" \
  --build-id "${build_id}" \
  --channel "${channel}" \
  --output "${output}"
