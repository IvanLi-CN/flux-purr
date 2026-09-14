#!/usr/bin/env bash
set -euo pipefail

if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi

COMMON_FEATURES="esp32s3,web_serial,net_http,frontpanel-key-test,buzzer-test"

lint_xtensa() {
  if [[ "$1" == "default" ]]; then
    cargo +esp clippy --locked \
      --manifest-path firmware/Cargo.toml \
      --target xtensa-esp32s3-none-elf \
      --bin flux-purr \
      -- -D warnings \
         -D clippy::too_many_lines \
         -D clippy::too_many_arguments \
         -D clippy::excessive_nesting
  else
    cargo +esp clippy --locked \
      --manifest-path firmware/Cargo.toml \
      --target xtensa-esp32s3-none-elf \
      --no-default-features \
      --features "$COMMON_FEATURES,pd-request-$1" \
      --bin flux-purr \
      -- -D warnings \
         -D clippy::too_many_lines \
         -D clippy::too_many_arguments \
         -D clippy::excessive_nesting
  fi
}

lint_xtensa default
for voltage in 12v 20v 28v; do
  lint_xtensa "$voltage"
done
