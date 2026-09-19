#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi

cargo clippy --locked --manifest-path firmware/Cargo.toml --all-targets -- -D warnings \
  -D clippy::too_many_lines -D clippy::too_many_arguments -D clippy::excessive_nesting
