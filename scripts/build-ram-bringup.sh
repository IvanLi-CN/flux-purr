#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo run --locked \
  --manifest-path firmware/Cargo.toml \
  --no-default-features \
  --example ram_calibration_asset -- --check

# Xtensa RAM execution cannot use LLVM jump tables in the JSON parser.
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C jump-tables=no"
source_sha="$(git rev-parse HEAD)"
build_id="${source_sha:0:16}"
FLUX_PURR_SOURCE_SHA="$source_sha" \
FLUX_PURR_BUILD_ID="$build_id" \
cargo +esp build --locked \
  --manifest-path firmware/ram-bringup/Cargo.toml \
  --target xtensa-esp32s3-none-elf \
  --target-dir firmware/ram-bringup/target \
  --release
