#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi

COMMON_FEATURES="esp32s3,host-preview,frontpanel-key-test"

cargo clippy --manifest-path firmware/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path firmware/Cargo.toml --all-targets --no-default-features --features "${COMMON_FEATURES},pd-request-12v" -- -D warnings
cargo clippy --manifest-path firmware/Cargo.toml --all-targets --no-default-features --features "${COMMON_FEATURES},pd-request-20v" -- -D warnings
cargo clippy --manifest-path firmware/Cargo.toml --all-targets --no-default-features --features "${COMMON_FEATURES},pd-request-28v" -- -D warnings
