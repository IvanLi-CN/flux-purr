#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi
cargo clippy --manifest-path firmware/Cargo.toml --all-targets --all-features -- -D warnings
