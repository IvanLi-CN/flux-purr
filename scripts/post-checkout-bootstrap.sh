#!/usr/bin/env bash
set -u

repo_root="$(git rev-parse --show-toplevel 2>/dev/null || printf '')"
if [[ -z "$repo_root" ]]; then
  exit 0
fi

bootstrap_script="$repo_root/scripts/bootstrap-dev.sh"
previous_ref="${1:-}"
next_ref="${2:-}"
branch_flag="${3:-0}"

if [[ ! -f "$bootstrap_script" ]]; then
  printf '[worktree-bootstrap][warn] bootstrap script missing: %s\n' "$bootstrap_script" >&2
  printf 'Recovery: bun run bootstrap:dev\n' >&2
  exit 0
fi

if ! bash "$bootstrap_script" --auto --previous-ref "$previous_ref" --next-ref "$next_ref" --branch-flag "$branch_flag"; then
  printf '[worktree-bootstrap][warn] automatic bootstrap failed; checkout remains available\n' >&2
  printf 'Recovery: bun run bootstrap:dev\n' >&2
fi

exit 0
