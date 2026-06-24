#!/usr/bin/env bash
set -euo pipefail

canonical_dir() {
  cd "$1" && pwd -P
}

repo_root="$(canonical_dir "$(git rev-parse --show-toplevel)")"
tmp_root="$(canonical_dir "$(mktemp -d "${TMPDIR:-/tmp}/flux-purr-worktree-bootstrap.XXXXXX")")"
trap 'rm -rf "$tmp_root"' EXIT

fixture_repo="$tmp_root/fixture"
worktree_dir="$tmp_root/linked"
custom_repo="$tmp_root/custom-hooks"
legacy_repo="$tmp_root/legacy"
fake_bin="$tmp_root/fake-bin"
bun_log="$tmp_root/bun.log"
cargo_log="$tmp_root/cargo.log"

copy_repo() {
  local src="$1"
  local dest="$2"

  mkdir -p "$dest"
  rsync -a \
    --exclude '.git' \
    --exclude 'Cargo.lock' \
    --exclude 'target' \
    --exclude 'node_modules' \
    --exclude 'web/node_modules' \
    --exclude 'web/dist' \
    --exclude 'web/storybook-static' \
    --exclude 'web/playwright-report' \
    --exclude 'web/test-results' \
    --exclude 'firmware/target' \
    --exclude 'tools/flux-purr-devd/target' \
    --exclude '.tmp' \
    --exclude '.mcu-agentd' \
    "$src/" "$dest/"
}

init_repo() {
  local repo="$1"

  git -C "$repo" init -b main >/dev/null
  git -C "$repo" config user.name 'Codex Test'
  git -C "$repo" config user.email 'codex-test@example.com'
  git -C "$repo" add .
  LEFTHOOK=0 git -C "$repo" commit -m 'test fixture' >/dev/null
}

run_git_fixture() {
  GIT_LFS_SKIP_SMUDGE=1 git "$@"
}

assert_contains() {
  local file="$1"
  local needle="$2"

  if ! grep -Fq -- "$needle" "$file"; then
    printf 'expected %s to contain %s\n' "$file" "$needle" >&2
    exit 1
  fi
}

write_fake_bun() {
  mkdir -p "$fake_bin"
  cat > "$fake_bin/bun" <<'EOF_BUN'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\t%s\n' "$(pwd)" "$*" >> "${BUN_LOG:?}"
if [[ "${1:-}" == "install" ]]; then
  os_arch="$(uname | tr '[:upper:]' '[:lower:]')"
  cpu_arch="$(uname -m | sed 's/aarch64/arm64/;s/x86_64/x64/')"
  native_dir="node_modules/lefthook-${os_arch}-${cpu_arch}/bin"
  mkdir -p "$native_dir" node_modules/.bin
  cat > "$native_dir/lefthook" <<'EOF_LEFTHOOK'
#!/usr/bin/env bash
set -euo pipefail
subcommand="${1:-}"
shift || true

case "$subcommand" in
install)
  common_dir="$(git rev-parse --path-format=absolute --git-common-dir)"
  mkdir -p "$common_dir/hooks"
  for name in pre-commit commit-msg pre-push post-checkout; do
    cat > "$common_dir/hooks/$name" <<HOOK
#!/bin/sh
call_lefthook() {
  if [ -n "\${LEFTHOOK_BIN:-}" ] && [ -x "\${LEFTHOOK_BIN}" ]; then
    "\${LEFTHOOK_BIN}" "\$@"
  fi
}
call_lefthook run "$name" "\$@"
HOOK
    chmod +x "$common_dir/hooks/$name"
  done
  exit 0
  ;;
run)
  hook_name="${1:-}"
  shift || true
  printf '%s\t%s %s\n' "$(pwd)" "$hook_name" "$*" >> "${LEFTHOOK_LOG:?}"
  if [[ "$hook_name" == "post-checkout" && -f scripts/post-checkout-bootstrap.sh ]]; then
    bash scripts/post-checkout-bootstrap.sh "$@"
  fi
  exit 0
  ;;
*)
  printf 'fake lefthook does not support subcommand: %s\n' "$subcommand" >&2
  exit 1
  ;;
esac
EOF_LEFTHOOK
  chmod +x "$native_dir/lefthook"
  if [[ -f package.json ]] && grep -Fq '"name": "flux-purr-web"' package.json; then
    mkdir -p node_modules/playwright
  fi
fi
exit 0
EOF_BUN
  chmod +x "$fake_bin/bun"

  cat > "$fake_bin/bunx" <<'EOF_BUNX'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\tbunx %s\n' "$(pwd)" "$*" >> "${BUN_LOG:?}"
exit 0
EOF_BUNX
  chmod +x "$fake_bin/bunx"
}

write_fake_cargo() {
  mkdir -p "$fake_bin"
  cat > "$fake_bin/cargo" <<'EOF_CARGO'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\t%s\n' "$(pwd)" "$*" >> "${CARGO_LOG:?}"
if [[ "${1:-}" == "+esp" && "${2:-}" == "--version" ]]; then
  exit 0
fi
if [[ "${1:-}" == "fetch" ]]; then
  exit 0
fi
exit 0
EOF_CARGO
  chmod +x "$fake_bin/cargo"
}

write_fake_jq() {
  mkdir -p "$fake_bin"
  cat > "$fake_bin/jq" <<'EOF_JQ'
#!/usr/bin/env bash
set -euo pipefail
exit 0
EOF_JQ
  chmod +x "$fake_bin/jq"
}

copy_repo "$repo_root" "$fixture_repo"
init_repo "$fixture_repo"
write_fake_bun
write_fake_cargo
write_fake_jq

export BUN_LOG="$bun_log"
export CARGO_LOG="$cargo_log"
export LEFTHOOK_LOG="$tmp_root/lefthook.log"
export HOME="$tmp_root/home"
mkdir -p "$HOME/.cache/ms-playwright"

(
  cd "$fixture_repo"
  PATH="$fake_bin:$PATH" bash scripts/bootstrap-dev.sh --manual >/dev/null
)

hooks_dir="$(git -C "$fixture_repo" rev-parse --path-format=absolute --git-common-dir)/hooks"
test -x "$hooks_dir/pre-commit"
test -x "$hooks_dir/commit-msg"
test -x "$hooks_dir/pre-push"
test -x "$hooks_dir/post-checkout"

assert_contains "$bun_log" "$fixture_repo"$'\t''install --frozen-lockfile'
assert_contains "$bun_log" "$fixture_repo"$'\t''install --cwd web --frozen-lockfile'
assert_contains "$cargo_log" $'\t''fetch --manifest-path '
assert_contains "$cargo_log" $'\t''+esp fetch --manifest-path '
if [[ -e "$fixture_repo/Cargo.lock" ]]; then
  printf 'expected bootstrap prewarm to avoid writing Cargo.lock into the real checkout\n' >&2
  exit 1
fi

PATH="$fake_bin:$PATH" run_git_fixture -C "$fixture_repo" worktree add --detach "$worktree_dir" HEAD >/dev/null
assert_contains "$bun_log" "$worktree_dir"$'\t''install --frozen-lockfile'
assert_contains "$bun_log" "$worktree_dir"$'\t''install --cwd web --frozen-lockfile'
if [[ -e "$worktree_dir/Cargo.lock" ]]; then
  printf 'expected linked worktree bootstrap to avoid writing Cargo.lock into the real checkout\n' >&2
  exit 1
fi

before_bun_lines="$(wc -l < "$bun_log")"
PATH="$fake_bin:$PATH" run_git_fixture -C "$worktree_dir" checkout --detach HEAD >/dev/null
after_bun_lines="$(wc -l < "$bun_log")"
if [[ "$before_bun_lines" != "$after_bun_lines" ]]; then
  printf 'expected repeated checkout to skip bun install\n' >&2
  exit 1
fi

printf '\n# fixture comment\n' >> "$worktree_dir/web/bun.lock"
PATH="$fake_bin:$PATH" run_git_fixture -C "$worktree_dir" checkout --detach HEAD >/dev/null
assert_contains "$bun_log" "$worktree_dir"$'\t''install --cwd web --frozen-lockfile'

(
  cd "$worktree_dir"
  "$hooks_dir/commit-msg" .git/COMMIT_EDITMSG >/dev/null 2>&1 || true
)
assert_contains "$LEFTHOOK_LOG" "$worktree_dir"$'\t''commit-msg'

copy_repo "$repo_root" "$custom_repo"
init_repo "$custom_repo"
mkdir -p "$custom_repo/.custom-hooks"
cat > "$custom_repo/.custom-hooks/post-checkout" <<'EOF_HOOK'
#!/bin/sh
echo custom-hook
EOF_HOOK
chmod +x "$custom_repo/.custom-hooks/post-checkout"
git -C "$custom_repo" config core.hooksPath .custom-hooks
(
  cd "$custom_repo"
  PATH="$fake_bin:$PATH" bun install --frozen-lockfile >/dev/null
  PATH="$fake_bin:$PATH" bash scripts/install-hooks.sh >/dev/null
  PATH="$fake_bin:$PATH" bash scripts/install-hooks.sh >/dev/null
)
configured_hooks_path="$(git -C "$custom_repo" config --local --get core.hooksPath)"
configured_hooks_abs="$(cd "$custom_repo" && cd "$configured_hooks_path" && pwd -P)"
test -x "$configured_hooks_abs/post-checkout"
if ! grep -Fq 'custom-hook' "$configured_hooks_abs/post-checkout" && ! grep -Fq 'legacy-hooks' "$configured_hooks_abs/post-checkout"; then
  printf 'expected chained custom post-checkout behavior to be preserved\n' >&2
  exit 1
fi

copy_repo "$repo_root" "$legacy_repo"
init_repo "$legacy_repo"
rm -f "$legacy_repo/scripts/bootstrap-dev.sh" "$legacy_repo/scripts/post-checkout-bootstrap.sh"
git -C "$legacy_repo" add -A
LEFTHOOK=0 git -C "$legacy_repo" commit -m 'legacy fixture' >/dev/null
legacy_worktree="$tmp_root/legacy-linked"
(
  cd "$legacy_repo"
  PATH="$fake_bin:$PATH" bun install --frozen-lockfile >/dev/null
  PATH="$fake_bin:$PATH" bash scripts/install-hooks.sh >/dev/null
)
PATH="$fake_bin:$PATH" run_git_fixture -C "$legacy_repo" worktree add --detach "$legacy_worktree" HEAD >/dev/null
PATH="$fake_bin:$PATH" run_git_fixture -C "$legacy_worktree" checkout --detach HEAD^ >/dev/null
PATH="$fake_bin:$PATH" run_git_fixture -C "$legacy_worktree" checkout --detach HEAD@{1} >/dev/null

printf 'worktree bootstrap smoke passed\n'
