#!/usr/bin/env bash
set -euo pipefail

root_dir="$(git rev-parse --show-toplevel)"
git fetch --tags --force >/dev/null 2>&1 || true

fw_manifest="${root_dir}/firmware/Cargo.toml"
manifest_version="$(grep -m1 '^version[[:space:]]*=' "$fw_manifest" | sed -E 's/.*"([0-9]+\.[0-9]+\.[0-9]+)".*/\1/')"

if [[ -z "${manifest_version:-}" ]]; then
  echo "Failed to parse firmware version from ${fw_manifest}" >&2
  exit 1
fi

base_version="$(
  {
    git tag --list 'fw/v[0-9]*.[0-9]*.[0-9]*' | sed -E 's#^fw/v([0-9]+)\.([0-9]+)\.([0-9]+)$#\1 \2 \3#'
    printf '%s\n' "${manifest_version}" | sed -E 's#^([0-9]+)\.([0-9]+)\.([0-9]+)$#\1 \2 \3#'
  } | awk 'NF == 3 { printf "%010d %010d %010d %d.%d.%d\n", $1, $2, $3, $1, $2, $3 }' | sort | tail -n1 | awk '{ print $4 }'
)"

release_level="${RELEASE_LEVEL:-patch}"
release_channel="${RELEASE_CHANNEL:-stable}"

major="${base_version%%.*}"
rest="${base_version#*.}"
minor="${rest%%.*}"
patch="${rest##*.}"

case "${release_level}" in
  major)
    major="$((major + 1))"
    minor=0
    patch=0
    ;;
  minor)
    minor="$((minor + 1))"
    patch=0
    ;;
  patch)
    patch="$((patch + 1))"
    ;;
  *)
    echo "Unsupported RELEASE_LEVEL=${release_level}" >&2
    exit 1
    ;;
esac

effective="${major}.${minor}.${patch}"
if [[ "${release_channel}" == "rc" ]]; then
  if [[ -n "${GITHUB_SHA:-}" ]]; then
    short_sha="${GITHUB_SHA:0:7}"
  elif short_sha="$(git rev-parse --short=7 HEAD 2>/dev/null)"; then
    short_sha="${short_sha:0:7}"
  else
    short_sha="local000"
  fi
  tag="fw/v${effective}-rc.${short_sha}"
else
  tag="fw/v${effective}"
fi

echo "FW_EFFECTIVE_VERSION=${effective}" >> "${GITHUB_ENV:-/dev/stdout}"
echo "FW_TAG=${tag}" >> "${GITHUB_ENV:-/dev/stdout}"
echo "Computed FW_TAG=${tag} (base ${base_version}, level ${release_level}, channel ${release_channel})"
