#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi
cargo fmt --manifest-path firmware/Cargo.toml --all -- --check
