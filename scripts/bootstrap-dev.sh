#!/usr/bin/env bash
set -euo pipefail

mode=""
previous_ref=""
next_ref=""
branch_flag="0"

usage() {
  cat <<'USAGE'
Usage:
  bash scripts/bootstrap-dev.sh --manual
  bash scripts/bootstrap-dev.sh --auto --previous-ref <sha> --next-ref <sha> --branch-flag <0|1>
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manual)
      mode="manual"
      shift
      ;;
    --auto)
      mode="auto"
      shift
      ;;
    --previous-ref)
      previous_ref="${2:-}"
      shift 2
      ;;
    --next-ref)
      next_ref="${2:-}"
      shift 2
      ;;
    --branch-flag)
      branch_flag="${2:-0}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      printf 'bootstrap-dev: unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$mode" ]]; then
  usage >&2
  exit 1
fi

repo_root="$(git rev-parse --show-toplevel)"
repo_root="$(cd "$repo_root" && pwd -P)"
git_dir="$(git rev-parse --path-format=absolute --git-dir)"
common_dir="$(git rev-parse --path-format=absolute --git-common-dir)"
stamp_dir="$git_dir/flux-purr-bootstrap"
stamp_file="$stamp_dir/state"
zero_oid="0000000000000000000000000000000000000000"

mkdir -p "$stamp_dir"

log() {
  printf '[worktree-bootstrap] %s\n' "$*"
}

warn() {
  printf '[worktree-bootstrap][warn] %s\n' "$*" >&2
}

cleanup_dir() {
  local path="$1"

  if [[ -n "$path" && -d "$path" ]]; then
    rm -rf "$path"
  fi
}

hash_file() {
  local path="$1"

  if [[ ! -f "$path" ]]; then
    printf 'missing'
    return 0
  fi
  shasum -a 256 "$path" | awk '{print $1}'
}

hash_glob() {
  local paths=("$@")
  local existing=()
  local path

  for path in "${paths[@]}"; do
    if [[ -f "$path" ]]; then
      existing+=("$path")
    fi
  done

  if [[ ${#existing[@]} -eq 0 ]]; then
    printf 'missing'
    return 0
  fi

  shasum -a 256 "${existing[@]}" | shasum -a 256 | awk '{print $1}'
}

prepare_cargo_prewarm_workspace() {
  local temp_root

  temp_root="$(mktemp -d "${TMPDIR:-/tmp}/flux-purr-cargo-prewarm.XXXXXX")"

  cp "$repo_root/Cargo.toml" "$temp_root/Cargo.toml"
  if [[ -f "$repo_root/rust-toolchain.toml" ]]; then
    cp "$repo_root/rust-toolchain.toml" "$temp_root/rust-toolchain.toml"
  fi
  if [[ -d "$repo_root/.cargo" ]]; then
    mkdir -p "$temp_root/.cargo"
    cp -R "$repo_root/.cargo/." "$temp_root/.cargo/"
  fi

  mkdir -p "$temp_root/firmware" "$temp_root/tools"
  cp -R "$repo_root/firmware" "$temp_root/"
  cp -R "$repo_root/tools/flux-purr-devd" "$temp_root/tools/"
  cleanup_dir "$temp_root/firmware/target"
  cleanup_dir "$temp_root/tools/flux-purr-devd/target"

  printf '%s\n' "$temp_root"
}

current_root_lock="$(hash_file "$repo_root/bun.lock")"
current_web_lock="$(hash_file "$repo_root/web/bun.lock")"
current_cargo_state="$(hash_glob "$repo_root/Cargo.toml" "$repo_root/firmware/Cargo.toml" "$repo_root/tools/flux-purr-devd/Cargo.toml" "$repo_root/rust-toolchain.toml")"

saved_root_lock=""
saved_web_lock=""
saved_cargo_state=""
if [[ -f "$stamp_file" ]]; then
  # shellcheck disable=SC1090
  source "$stamp_file"
fi

current_is_main=0
if [[ "$git_dir" == "$common_dir" ]]; then
  current_is_main=1
fi

should_run_auto=1
if [[ "$mode" == "auto" ]]; then
  if [[ "$current_is_main" -eq 1 ]]; then
    should_run_auto=0
    log "skip auto bootstrap on main worktree"
  fi
fi

if [[ "$should_run_auto" -eq 0 ]]; then
  exit 0
fi

run_root_install=0
run_web_install=0
run_cargo_fetch=0

if [[ "$mode" == "manual" || "$saved_root_lock" != "$current_root_lock" ]]; then
  run_root_install=1
fi
if [[ "$mode" == "manual" || "$saved_web_lock" != "$current_web_lock" ]]; then
  run_web_install=1
fi
if [[ "$mode" == "manual" || "$saved_cargo_state" != "$current_cargo_state" ]]; then
  run_cargo_fetch=1
fi

missing_prereq=0
need_playwright_browser=0
bootstrap_warning=0

check_command() {
  local name="$1"
  local help="$2"

  if command -v "$name" >/dev/null 2>&1; then
    log "detected system prerequisite: $name"
    return 0
  fi

  warn "missing system prerequisite: $name"
  warn "fix: $help"
  missing_prereq=1
  return 1
}

warn_bootstrap_failure() {
  local layer="$1"
  local help="$2"

  warn "$layer"
  warn "fix: $help"
  bootstrap_warning=1
}

check_command bun 'install Bun from https://bun.sh or your package manager, then rerun `bun run bootstrap:dev`' || true
check_command rustup 'install Rust via https://rustup.rs, then rerun `bun run bootstrap:dev`' || true
check_command cargo 'ensure Rust toolchain is on PATH, then rerun `bun run bootstrap:dev`' || true
check_command jq 'install jq via your package manager, then rerun `bun run bootstrap:dev`' || true

if command -v cargo >/dev/null 2>&1; then
  if cargo +esp --version >/dev/null 2>&1; then
    log "detected system prerequisite: cargo +esp"
  else
    warn "missing system prerequisite: cargo +esp"
    warn "fix: install espup / Xtensa toolchain, then rerun `bun run bootstrap:dev`"
    missing_prereq=1
  fi
fi

if [[ -d "$repo_root/web/node_modules/playwright" || -d "$repo_root/web/node_modules/.cache/ms-playwright" ]]; then
  :
fi
if [[ -d "$HOME/Library/Caches/ms-playwright" || -d "$HOME/.cache/ms-playwright" ]]; then
  log "detected Playwright browser cache"
else
  warn "Playwright Chromium cache not detected"
  warn "fix: (cd web && bunx playwright install chromium) after bootstrap if you need local e2e"
  need_playwright_browser=1
fi

if [[ "$run_root_install" -eq 1 ]]; then
  if command -v bun >/dev/null 2>&1; then
    log "install root dependencies"
    (cd "$repo_root" && bun install --frozen-lockfile)
  else
    warn "skip root dependency install because bun is missing"
  fi
else
  log "root dependencies already up to date"
fi

if [[ "$run_web_install" -eq 1 ]]; then
  if command -v bun >/dev/null 2>&1; then
    log "install web dependencies"
    (cd "$repo_root" && bun install --cwd web --frozen-lockfile)
  else
    warn "skip web dependency install because bun is missing"
  fi
else
  log "web dependencies already up to date"
fi

if [[ "$run_cargo_fetch" -eq 1 ]]; then
  if command -v cargo >/dev/null 2>&1; then
    cargo_prewarm_workspace=""
    log "prewarm Cargo dependencies"
    cargo_prewarm_workspace="$(prepare_cargo_prewarm_workspace)"
    if ! cargo fetch --manifest-path "$cargo_prewarm_workspace/Cargo.toml"; then
      warn_bootstrap_failure \
        "Cargo prewarm failed for the temporary workspace snapshot" \
        "check Cargo network/toolchain health, then rerun \`bun run bootstrap:dev\`"
    fi
    if cargo +esp --version >/dev/null 2>&1; then
      if ! cargo +esp fetch --manifest-path "$cargo_prewarm_workspace/Cargo.toml" --target xtensa-esp32s3-none-elf; then
        warn_bootstrap_failure \
          "Cargo prewarm failed for the Xtensa firmware target snapshot" \
          "check the Xtensa toolchain install, then rerun \`bun run bootstrap:dev\`"
      fi
    else
      log "skip Xtensa Cargo prewarm because cargo +esp is unavailable"
    fi
    cleanup_dir "$cargo_prewarm_workspace"
  else
    warn "skip Cargo fetch because cargo is missing"
  fi
else
  log "Cargo dependency state already up to date"
fi

if [[ "$mode" == "auto" && "$run_root_install" -eq 0 && "$run_web_install" -eq 0 && "$run_cargo_fetch" -eq 0 ]]; then
  log "linked worktree healthy; no repo-managed dependency changes detected"
fi

log "refresh shared hooks"
(cd "$repo_root" && bash scripts/install-hooks.sh)

cat > "$stamp_file" <<EOF_STATE
saved_root_lock="$current_root_lock"
saved_web_lock="$current_web_lock"
saved_cargo_state="$current_cargo_state"
EOF_STATE

if [[ "$missing_prereq" -eq 1 || "$need_playwright_browser" -eq 1 || "$bootstrap_warning" -eq 1 ]]; then
  log "bootstrap finished with warnings"
else
  log "bootstrap finished"
fi
