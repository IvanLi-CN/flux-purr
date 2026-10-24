#!/usr/bin/env bash
set -euo pipefail
if [[ ! -d firmware ]]; then
  echo "firmware/ not found; skipping"
  exit 0
fi

check_factory_app_size() {
  local elf_path="$1"
  local output_path log_path size_line app_size partition_size
  if [[ ! -f "$elf_path" ]]; then
    echo "firmware ELF not found: $elf_path" >&2
    exit 1
  fi
  if ! command -v espflash >/dev/null 2>&1; then
    echo "espflash is required for the factory-app image-size check" >&2
    exit 1
  fi

  output_path="$(mktemp "${TMPDIR:-/tmp}/flux-purr-factory-image.XXXXXX")"
  log_path="$(mktemp "${TMPDIR:-/tmp}/flux-purr-factory-image.XXXXXX")"
  trap 'rm -f "$output_path" "$log_path"' RETURN
  espflash save-image \
    --chip esp32s3 \
    --flash-size 4mb \
    --flash-mode dio \
    --flash-freq 40m \
    --xtal-freq 40m \
    --partition-table firmware/partitions.csv \
    --target-app-partition factory \
    --merge \
    --skip-padding \
    "$elf_path" "$output_path" | tee "$log_path"
  size_line="$(grep -E '^App/part\. size:' "$log_path" || true)"
  if [[ -z "$size_line" ]]; then
    echo "espflash did not report the factory-app image size" >&2
    exit 1
  fi
  app_size="$(sed -E 's/^App\/part\. size:[[:space:]]*([0-9,]+)\/[0-9,]+ bytes.*/\1/; s/,//g' <<<"$size_line")"
  partition_size="$(sed -E 's/^App\/part\. size:[[:space:]]*[0-9,]+\/([0-9,]+) bytes.*/\1/; s/,//g' <<<"$size_line")"
  if [[ ! "$app_size" =~ ^[0-9]+$ || ! "$partition_size" =~ ^[0-9]+$ ]]; then
    echo "could not parse espflash factory-app size: $size_line" >&2
    exit 1
  fi
  if (( app_size > partition_size )); then
    echo "factory-app image exceeds partition: ${app_size}B > ${partition_size}B" >&2
    exit 1
  fi
  echo "factory-app image fits partition: ${app_size}B/${partition_size}B"
}

cargo +esp build --locked \
  --manifest-path firmware/Cargo.toml \
  --target xtensa-esp32s3-none-elf \
  --target-dir firmware/target \
  --release
check_factory_app_size firmware/target/xtensa-esp32s3-none-elf/release/flux-purr
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
