#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d tools/flux-purr-devd ]]; then
  echo "tools/flux-purr-devd not found; skipping"
  exit 0
fi
cargo test --manifest-path tools/flux-purr-devd/Cargo.toml
