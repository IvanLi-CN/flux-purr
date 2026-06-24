#!/usr/bin/env bash
set -euo pipefail

hook_name="${1:?hook name is required}"
shift || true

repo_root="$(git rev-parse --show-toplevel)"
repo_root="$(cd "$repo_root" && pwd -P)"

select_lefthook_bin() {
  local target_root="$1"
  local os_arch
  local cpu_arch
  local candidate

  os_arch="$(uname | tr '[:upper:]' '[:lower:]')"
  cpu_arch="$(uname -m | sed 's/aarch64/arm64/;s/x86_64/x64/')"
  candidate="$target_root/node_modules/lefthook-${os_arch}-${cpu_arch}/bin/lefthook"
  if [[ -x "$candidate" ]]; then
    printf '%s\n' "$candidate"
    return 0
  fi

  candidate="$target_root/node_modules/.bin/lefthook"
  if [[ -x "$candidate" ]]; then
    printf '%s\n' "$candidate"
    return 0
  fi

  return 1
}

lefthook_bin="${LEFTHOOK_BIN:-}"
if [[ -n "$lefthook_bin" && ! -x "$lefthook_bin" ]]; then
  lefthook_bin=""
fi
if [[ -z "$lefthook_bin" ]]; then
  lefthook_bin="$(select_lefthook_bin "$repo_root" || true)"
fi

if [[ -z "$lefthook_bin" ]]; then
  printf 'run-lefthook-hook: lefthook is not installed in %s\n' "$repo_root" >&2
  exit 0
fi

LEFTHOOK_BIN="$lefthook_bin" exec "$lefthook_bin" run "$hook_name" "$@"

