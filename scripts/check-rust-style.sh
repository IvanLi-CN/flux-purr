#!/usr/bin/env bash
set -euo pipefail

if [[ ! -d tools/rust-style-check ]]; then
  echo "tools/rust-style-check not found" >&2
  exit 1
fi

cargo clippy --locked --manifest-path tools/rust-style-check/Cargo.toml --all-targets -- \
  -D warnings \
  -D clippy::too_many_lines \
  -D clippy::too_many_arguments \
  -D clippy::excessive_nesting
cargo test --locked --manifest-path tools/rust-style-check/Cargo.toml
cargo run --locked --manifest-path tools/rust-style-check/Cargo.toml --bin rust-style-check
