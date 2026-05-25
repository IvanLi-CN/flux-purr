#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi
cargo +esp build \
  --manifest-path firmware/Cargo.toml \
  --target xtensa-esp32s3-none-elf \
  --target-dir firmware/target \
  --release
