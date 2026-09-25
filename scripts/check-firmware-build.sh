#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi
cargo +esp build --locked \
  --manifest-path firmware/Cargo.toml \
  --target xtensa-esp32s3-none-elf \
  --target-dir firmware/target \
  --release
bash scripts/check-firmware-boot-stack.sh

cargo +esp build --locked \
  --manifest-path firmware/Cargo.toml \
  --target xtensa-esp32s3-none-elf \
  --target-dir firmware/target-no-net \
  --no-default-features \
  --features "esp32s3,web_serial,frontpanel-key-test,buzzer-test" \
  --release
bash scripts/check-firmware-boot-stack.sh \
  firmware/target-no-net/xtensa-esp32s3-none-elf/release/flux-purr

cargo +esp build --locked \
  --package flux-purr-ram-bringup \
  --target xtensa-esp32s3-none-elf \
  --target-dir firmware/target \
  --release
bash scripts/check-ram-bringup-elf.sh \
  firmware/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-bringup
