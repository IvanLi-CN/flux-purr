#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi

COMMON_FEATURES="esp32s3,host-preview,frontpanel-key-test"

cargo clippy --locked --manifest-path firmware/Cargo.toml --all-targets -- -D warnings \
  -D clippy::too_many_lines -D clippy::too_many_arguments -D clippy::excessive_nesting
cargo clippy --locked --manifest-path firmware/Cargo.toml --all-targets --no-default-features --features "${COMMON_FEATURES},pd-request-12v" -- -D warnings \
  -D clippy::too_many_lines -D clippy::too_many_arguments -D clippy::excessive_nesting
cargo clippy --locked --manifest-path firmware/Cargo.toml --all-targets --no-default-features --features "${COMMON_FEATURES},pd-request-20v" -- -D warnings \
  -D clippy::too_many_lines -D clippy::too_many_arguments -D clippy::excessive_nesting
cargo clippy --locked --manifest-path firmware/Cargo.toml --all-targets --no-default-features --features "${COMMON_FEATURES},pd-request-28v" -- -D warnings \
  -D clippy::too_many_lines -D clippy::too_many_arguments -D clippy::excessive_nesting
