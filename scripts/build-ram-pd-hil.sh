#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-C jump-tables=no"
source_sha="$(git rev-parse HEAD)"
build_id="${source_sha:0:16}"
FLUX_PURR_SOURCE_SHA="$source_sha" \
FLUX_PURR_BUILD_ID="$build_id" \
cargo +esp build --locked \
  --manifest-path firmware/ram-pd-hil/Cargo.toml \
  --target xtensa-esp32s3-none-elf \
  --target-dir firmware/ram-pd-hil/target \
  --release

python3 firmware/ram-bringup/tools/check_ram_elf.py \
  firmware/ram-pd-hil/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-pd-hil \
  --json
