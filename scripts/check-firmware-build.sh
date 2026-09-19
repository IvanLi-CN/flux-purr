#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi
# The no-net variant still carries web_serial because the USB control plane is
# the developer/HIL transport. It deliberately omits only net_http.
COMMON_FEATURES="esp32s3,web_serial,net_http,frontpanel-key-test,buzzer-test"

build_firmware() {
  local target_dir="firmware/target"
  if [[ "$1" == "default" ]]; then
    cargo +esp build --locked \
      --manifest-path firmware/Cargo.toml \
      --target xtensa-esp32s3-none-elf \
      --target-dir "$target_dir" \
      --release
    return
  fi

  target_dir="firmware/target-$1"

  cargo +esp build --locked \
    --manifest-path firmware/Cargo.toml \
    --target xtensa-esp32s3-none-elf \
    --target-dir "$target_dir" \
    --no-default-features \
    --features "$COMMON_FEATURES,pd-request-$1" \
    --release
}

build_firmware default
bash scripts/check-firmware-boot-stack.sh
for voltage in 12v 20v 28v; do
  build_firmware "$voltage"
  bash scripts/check-firmware-boot-stack.sh \
    "firmware/target-$voltage/xtensa-esp32s3-none-elf/release/flux-purr"
done

cargo +esp build --locked \
  --manifest-path firmware/Cargo.toml \
  --target xtensa-esp32s3-none-elf \
  --target-dir firmware/target-no-net \
  --no-default-features \
  --features "esp32s3,web_serial,frontpanel-key-test,buzzer-test,pd-request-12v" \
  --release
bash scripts/check-firmware-boot-stack.sh \
  firmware/target-no-net/xtensa-esp32s3-none-elf/release/flux-purr
