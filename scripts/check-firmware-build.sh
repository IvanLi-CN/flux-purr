#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi
COMMON_FEATURES="esp32s3,web_serial,net_http,frontpanel-key-test,buzzer-test"

build_firmware() {
  if [[ "$1" == "default" ]]; then
    cargo +esp build --locked \
      --manifest-path firmware/Cargo.toml \
      --target xtensa-esp32s3-none-elf \
      --target-dir firmware/target \
      --release
    return
  fi

  cargo +esp build --locked \
    --manifest-path firmware/Cargo.toml \
    --target xtensa-esp32s3-none-elf \
    --target-dir firmware/target \
    --no-default-features \
    --features "$COMMON_FEATURES,pd-request-$1" \
    --release
}

build_firmware default
for voltage in 12v 20v 28v; do
  build_firmware "$voltage"
done
