#!/usr/bin/env bash
set -euo pipefail

if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi

cargo +esp clippy --locked \
  --manifest-path firmware/Cargo.toml \
  --target xtensa-esp32s3-none-elf \
  --bin flux-purr \
  -- -D warnings \
     -D clippy::too_many_lines \
     -D clippy::too_many_arguments \
     -D clippy::excessive_nesting

cargo +esp clippy --locked \
  --package flux-purr-ram-bringup \
  --target xtensa-esp32s3-none-elf \
  --bin flux-purr-ram-bringup \
  -- -D warnings \
     -D clippy::too_many_lines \
     -D clippy::too_many_arguments \
     -D clippy::excessive_nesting
