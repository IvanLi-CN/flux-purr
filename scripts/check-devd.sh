#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d tools/flux-purr-devd ]]; then
  echo "tools/flux-purr-devd not found; skipping"
  exit 0
fi
python3 -m py_compile scripts/devd-hardware-smoke.py
cargo fmt --manifest-path tools/flux-purr-devd/Cargo.toml --all -- --check
cargo clippy --locked --manifest-path tools/flux-purr-devd/Cargo.toml --all-targets -- -D warnings \
  -D clippy::too_many_lines -D clippy::too_many_arguments -D clippy::excessive_nesting
cargo test --locked --manifest-path tools/flux-purr-devd/Cargo.toml
cargo test --locked --manifest-path tools/flux-purr-devd/Cargo.toml --test mock_smoke -- --exact
