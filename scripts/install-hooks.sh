#!/usr/bin/env bash
set -euo pipefail

main_root_key="flux-purr.worktree-bootstrap.main-root"
managed_marker="flux-purr hooks:install managed"

log() {
  printf 'hooks-install: %s\n' "$*"
}

canonical_dir() {
  cd -- "$1" && pwd -P
}

canonical_path() {
  local target="$1"
  local parent
  local base

  parent="$(dirname -- "$target")"
  base="$(basename -- "$target")"
  printf '%s/%s\n' "$(canonical_dir "$parent")" "$base"
}

resolve_git_path() {
  local path

  path="$(git rev-parse "$1")"
  case "$path" in
    /*) printf '%s\n' "$path" ;;
    *) canonical_path "$path" ;;
  esac
}

read_recorded_main_root() {
  local recorded_root

  recorded_root="$(git config --path --get "$main_root_key" 2>/dev/null || true)"
  if [[ -z "$recorded_root" || ! -d "$recorded_root" ]]; then
    return 1
  fi

  canonical_dir "$recorded_root"
}

record_main_root() {
  local target="$1"

  git -C "$target" config --local "$main_root_key" "$target"
}

discover_main_root() {
  local current_root="$1"
  local git_dir
  local common_dir
  local recorded_root=""
  local listed_root=""

  git_dir="$(resolve_git_path --git-dir)"
  common_dir="$(resolve_git_path --git-common-dir)"

  recorded_root="$(read_recorded_main_root || true)"
  if [[ -n "$recorded_root" ]]; then
    printf '%s\n' "$recorded_root"
    return 0
  fi

  if [[ "$git_dir" == "$common_dir" ]]; then
    printf '%s\n' "$current_root"
    return 0
  fi

  listed_root="$(git worktree list --porcelain 2>/dev/null | awk '
    index($0, "worktree ") == 1 {
      print substr($0, 10)
      exit
    }
  ')"
  if [[ -n "$listed_root" ]]; then
    listed_root="$(canonical_dir "$listed_root")"
    if [[ "$listed_root" != "$common_dir" ]]; then
      printf '%s\n' "$listed_root"
      return 0
    fi
  fi

  if [[ "$(basename -- "$common_dir")" == ".git" ]]; then
    canonical_dir "$common_dir/.."
    return 0
  fi

  return 1
}

shared_hooks_dir() {
  local common_dir

  common_dir="$(resolve_git_path --git-common-dir)"
  printf '%s/hooks\n' "$common_dir"
}

legacy_hooks_dir() {
  local hooks_dir

  hooks_dir="$(shared_hooks_dir)"
  printf '%s/.legacy-hooks\n' "$hooks_dir"
}

native_lefthook_bin() {
  local base_root="$1"
  local os_arch
  local cpu_arch
  local candidate

  os_arch="$(uname | tr '[:upper:]' '[:lower:]')"
  cpu_arch="$(uname -m | sed 's/aarch64/arm64/;s/x86_64/x64/')"
  candidate="$base_root/node_modules/lefthook-${os_arch}-${cpu_arch}/bin/lefthook"
  if [[ -x "$candidate" ]]; then
    printf '%s\n' "$candidate"
    return 0
  fi
  return 1
}

select_lefthook_bin() {
  local base_root="$1"
  local native_candidate=""
  local wrapper_candidate=""

  native_candidate="$(native_lefthook_bin "$base_root" || true)"
  if [[ -n "$native_candidate" ]]; then
    printf '%s\n' "$native_candidate"
    return 0
  fi

  wrapper_candidate="$base_root/node_modules/.bin/lefthook"
  if [[ -x "$wrapper_candidate" ]]; then
    printf '%s\n' "$wrapper_candidate"
    return 0
  fi

  return 1
}

resolve_hooks_dir() {
  local repo_root="$1"
  local configured_path="$2"
  local candidate

  case "$configured_path" in
    '') return 1 ;;
    /*) candidate="$configured_path" ;;
    *) candidate="$repo_root/$configured_path" ;;
  esac

  if [[ ! -d "$candidate" ]]; then
    return 1
  fi

  canonical_dir "$candidate"
}

list_configured_hooks() {
  local config_path="$1"

  awk '
    /^[[:space:]]*#/ { next }
    /^[^[:space:]][^:]*:[[:space:]]*$/ {
      key=$1
      sub(/:$/, "", key)
      if (key != "assert_lefthook_installed" && key != "colors" && key != "extends" && key != "min_version" && key != "no_tty" && key != "output" && key != "rc" && key != "skip_output" && key != "skip_lfs" && key != "source_dir") {
        print key
      }
    }
  ' "$config_path"
}

pin_hook_binary() {
  local hook_path="$1"
  shift
  local tmp_path="$hook_path.tmp"
  local hook_bin
  local escaped_bin
  local line

  {
    IFS= read -r first_line || first_line='#!/bin/sh'
    printf '%s\n' "$first_line"
    printf '# %s\n' "$managed_marker"
    printf 'if [ -n "${LEFTHOOK_BIN:-}" ] && [ ! -x "${LEFTHOOK_BIN}" ]; then\n'
    printf '  unset LEFTHOOK_BIN\n'
    printf 'fi\n'

    for hook_bin in "$@"; do
      if [[ -z "$hook_bin" ]]; then
        continue
      fi
      escaped_bin="$(printf '%s' "$hook_bin" | sed "s/'/'\\\\''/g")"
      printf "if [ -z \"\${LEFTHOOK_BIN:-}\" ] && [ -x '%s' ]; then\n" "$escaped_bin"
      printf "  LEFTHOOK_BIN='%s'\n" "$escaped_bin"
      printf '  export LEFTHOOK_BIN\n'
      printf 'fi\n'
    done

    printf '\n'
    while IFS= read -r line; do
      if [[ "$line" == "# $managed_marker" ]]; then
        continue
      fi
      printf '%s\n' "$line"
    done
  } < "$hook_path" > "$tmp_path"

  mv "$tmp_path" "$hook_path"
  chmod +x "$hook_path"
}

append_chained_hook() {
  local hook_path="$1"
  local hook_name="$2"
  local chained_hook="$3"
  local tmp_path="$hook_path.tmp"
  local escaped_hook

  escaped_hook="$(printf '%s' "$chained_hook" | sed "s/'/'\\\\''/g")"

  awk -v target="call_lefthook run \"$hook_name\" \"\$@\"" -v chained="$escaped_hook" '
    $0 == target {
      print $0
      print "lefthook_status=$?"
      print "if [ \"$lefthook_status\" -ne 0 ]; then"
      print "  exit \"$lefthook_status\""
      print "fi"
      print ""
      print "if [ -x '\''" chained "'\'' ]; then"
      print "  '\''" chained "'\'' \"$@\""
      print "fi"
      next
    }
    { print }
  ' "$hook_path" > "$tmp_path"

  mv "$tmp_path" "$hook_path"
  chmod +x "$hook_path"
}

install_passthrough_hook() {
  local hook_path="$1"
  local chained_hook="$2"
  local escaped_hook

  escaped_hook="$(printf '%s' "$chained_hook" | sed "s/'/'\\\\''/g")"

  cat > "$hook_path" <<HOOK
#!/bin/sh
# $managed_marker
if [ -x '$escaped_hook' ]; then
  '$escaped_hook' "\$@"
fi
HOOK

  chmod +x "$hook_path"
}

hook_is_repo_managed() {
  local hook_path="$1"

  if [[ ! -f "$hook_path" ]]; then
    return 1
  fi

  if grep -Fq "# $managed_marker" "$hook_path"; then
    return 0
  fi

  grep -Fq 'call_lefthook run "' "$hook_path"
}

snapshot_legacy_hooks() {
  local source_dir="$1"
  local snapshot_dir="$2"
  local previous_hook
  local hook_name

  mkdir -p "$snapshot_dir"
  find "$snapshot_dir" -mindepth 1 -maxdepth 1 -exec rm -rf {} +

  for previous_hook in "$source_dir"/*.old; do
    [[ -e "$previous_hook" ]] || continue
    [[ -x "$previous_hook" ]] || continue
    hook_name="$(basename -- "${previous_hook%.old}")"
    cp "$previous_hook" "$snapshot_dir/$hook_name"
    chmod +x "$snapshot_dir/$hook_name"
  done

  for previous_hook in "$source_dir"/*; do
    [[ -e "$previous_hook" ]] || continue
    [[ -x "$previous_hook" ]] || continue
    case "$previous_hook" in
      *.old) continue ;;
    esac
    hook_name="$(basename -- "$previous_hook")"
    if [[ -e "$snapshot_dir/$hook_name" ]]; then
      continue
    fi
    cp "$previous_hook" "$snapshot_dir/$hook_name"
    chmod +x "$snapshot_dir/$hook_name"
  done
}

snapshot_backup_hooks() {
  local source_dir="$1"
  local snapshot_dir="$2"
  local previous_hook
  local hook_name

  mkdir -p "$snapshot_dir"
  find "$snapshot_dir" -mindepth 1 -maxdepth 1 -exec rm -rf {} +

  for previous_hook in "$source_dir"/*.old; do
    [[ -e "$previous_hook" ]] || continue
    [[ -x "$previous_hook" ]] || continue
    hook_name="$(basename -- "${previous_hook%.old}")"
    cp "$previous_hook" "$snapshot_dir/$hook_name"
    chmod +x "$snapshot_dir/$hook_name"
  done
}

snapshot_has_hooks() {
  local snapshot_dir="$1"
  local snapshot_hook

  for snapshot_hook in "$snapshot_dir"/*; do
    [[ -e "$snapshot_hook" ]] || continue
    return 0
  done
  return 1
}

if ! git rev-parse --show-toplevel >/dev/null 2>&1; then
  log 'skip (not inside a Git worktree)'
  exit 0
fi

current_root="$(canonical_dir "$(git rev-parse --show-toplevel)")"
main_root="$(discover_main_root "$current_root" || true)"
main_bin=''
current_bin="$(select_lefthook_bin "$current_root" || true)"

if [[ -n "$main_root" ]]; then
  record_main_root "$main_root"
  main_bin="$(select_lefthook_bin "$main_root" || true)"
fi

install_bin="$current_bin"
if [[ -z "$install_bin" ]]; then
  install_bin="$main_bin"
fi

if [[ -z "$install_bin" ]]; then
  if [[ -n "$main_root" && "$current_root" != "$main_root" ]]; then
    log "skip (run bun install once from main worktree: $main_root)"
    exit 0
  fi

  if [[ -n "$main_root" ]]; then
    log "skip (repo-local lefthook binary missing in: $main_root)"
  else
    log 'skip (repo-local lefthook binary missing)'
  fi
  exit 0
fi

if [[ -z "$main_root" ]]; then
  main_root="$current_root"
  record_main_root "$main_root"
fi

hooks_dir="$(shared_hooks_dir)"
snapshot_dir="$(legacy_hooks_dir)"
previous_hooks_value="$(git -C "$main_root" config --local --path --get core.hooksPath 2>/dev/null || true)"
previous_hooks_dir=''
previous_hooks_origin=''
if [[ -n "$previous_hooks_value" ]]; then
  previous_hooks_dir="$(resolve_hooks_dir "$main_root" "$previous_hooks_value" || true)"
  if [[ "$previous_hooks_dir" == "$hooks_dir" ]]; then
    previous_hooks_dir=''
  fi
fi
if [[ -z "$previous_hooks_dir" ]] && snapshot_has_hooks "$snapshot_dir"; then
  previous_hooks_dir="$snapshot_dir"
  previous_hooks_origin='shared-backup'
fi
if [[ -n "$previous_hooks_dir" ]]; then
  if [[ "$previous_hooks_dir" != "$snapshot_dir" ]]; then
    snapshot_legacy_hooks "$previous_hooks_dir" "$snapshot_dir"
    previous_hooks_dir="$snapshot_dir"
    previous_hooks_origin='custom'
  fi
fi

git -C "$main_root" config --local core.hooksPath "$hooks_dir"

(
  cd "$current_root"
  "$install_bin" install --force
)

if [[ -z "$previous_hooks_dir" ]]; then
  snapshot_backup_hooks "$hooks_dir" "$snapshot_dir"
  if snapshot_has_hooks "$snapshot_dir"; then
    previous_hooks_dir="$snapshot_dir"
    previous_hooks_origin='shared-backup'
  fi
fi

fallback_bin=''
if [[ -n "$main_bin" && "$main_bin" != "$install_bin" ]]; then
  fallback_bin="$main_bin"
fi

if [[ -f "$current_root/lefthook.yml" ]]; then
  list_configured_hooks "$current_root/lefthook.yml" | while IFS= read -r hook_name; do
    hook_path="$(git rev-parse --git-path "hooks/$hook_name")"
    previous_hook=''
    case "$hook_path" in
      /*) : ;;
      *) hook_path="$current_root/$hook_path" ;;
    esac
    if [[ -f "$hook_path" ]]; then
      pin_hook_binary "$hook_path" "$install_bin" "$fallback_bin"
      if [[ -n "$previous_hooks_dir" && -x "$previous_hooks_dir/$hook_name" ]]; then
        previous_hook="$previous_hooks_dir/$hook_name"
        if [[ "$previous_hooks_origin" == "shared-backup" ]] && hook_is_repo_managed "$previous_hook"; then
          continue
        fi
        append_chained_hook "$hook_path" "$hook_name" "$previous_hook"
      fi
    fi
  done
fi

if [[ -n "$previous_hooks_dir" && -d "$previous_hooks_dir" ]]; then
  for previous_hook in "$previous_hooks_dir"/*; do
    [[ -e "$previous_hook" ]] || continue
    [[ -x "$previous_hook" ]] || continue
    if [[ "$previous_hooks_origin" == "shared-backup" ]] && hook_is_repo_managed "$previous_hook"; then
      continue
    fi
    hook_name="$(basename -- "$previous_hook")"
    shared_hook="$hooks_dir/$hook_name"
    if [[ -f "$shared_hook" ]]; then
      continue
    fi
    install_passthrough_hook "$shared_hook" "$previous_hook"
  done
fi

log "installed shared hooks in $hooks_dir"
