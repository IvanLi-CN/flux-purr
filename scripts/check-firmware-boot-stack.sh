#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
elf_path="${1:-${repo_root}/firmware/target/xtensa-esp32s3-none-elf/release/flux-purr}"
min_boot_stack_headroom_bytes="${FLUX_PURR_BOOT_STACK_MIN_HEADROOM_BYTES:-32768}"

if [[ ! -f "$elf_path" ]]; then
  echo "firmware ELF not found: $elf_path" >&2
  exit 1
fi

nm_bin="$(command -v xtensa-esp32s3-elf-nm || true)"
objdump_bin="$(command -v xtensa-esp32s3-elf-objdump || true)"
if [[ -z "$nm_bin" || -z "$objdump_bin" ]]; then
  echo "Xtensa nm/objdump tools are required for the boot stack check" >&2
  exit 1
fi

parse_number() {
  local value="$1"
  if [[ "$value" == 0x* ]]; then
    echo "$((16#${value#0x}))"
  else
    echo "$value"
  fi
}

frame_bytes_for_symbol() {
  local address="$1"
  local disassembly entry_value dynamic_value frame_bytes
  disassembly="$("$objdump_bin" -d --demangle --start-address="0x${address}" \
    --stop-address="$((16#${address} + 128))" "$elf_path")"
  entry_value="$(printf '%s\n' "$disassembly" | awk '
    /entry.*a1/ {
      value = $0
      sub(/^.*entry[[:space:]]+a1,[[:space:]]*/, "", value)
      print value
      exit
    }
  ')"
  if [[ -z "$entry_value" ]]; then
    echo "failed to parse task stack frame at 0x${address}" >&2
    exit 1
  fi
  frame_bytes="$(parse_number "$entry_value")"
  dynamic_value="$(printf '%s\n' "$disassembly" | awk '
    /entry.*a1/ { seen = 1; next }
    seen && /l32r/ && /\([0-9a-f]+ </ {
      value = $0
      sub(/^.*\(/, "", value)
      sub(/ <.*$/, "", value)
    }
    seen && /movsp.*a1/ {
      print value
      exit
    }
  ')"
  if [[ -n "$dynamic_value" ]]; then
    frame_bytes=$((frame_bytes + 16#${dynamic_value}))
  fi
  echo "$frame_bytes"
}

stack_start="$("$nm_bin" -n "$elf_path" | awk '$3 == "_stack_start_cpu0" { print $1; exit }')"
stack_end="$("$nm_bin" -n "$elf_path" | awk '$3 == "_stack_end_cpu0" { print $1; exit }')"
if [[ -z "$stack_start" || -z "$stack_end" ]]; then
  echo "failed to resolve CPU0 stack symbols" >&2
  exit 1
fi
stack_capacity_bytes=$((16#$stack_start - 16#$stack_end))
max_boot_stack_bytes="${FLUX_PURR_BOOT_STACK_MAX_BYTES:-$((stack_capacity_bytes - min_boot_stack_headroom_bytes))}"
if (( max_boot_stack_bytes < 0 )); then
  echo "CPU0 stack capacity is below the required headroom" >&2
  exit 1
fi

runtime_loop_symbol="$("$nm_bin" -Sn --demangle "$elf_path" | awk '
  /flux_purr::runtime::runtime_loop::run_runtime_loop::\{closure#0\}$/ {
    print $1 " runtime_loop"
    exit
  }
')"
if [[ -z "$runtime_loop_symbol" ]]; then
  echo "failed to resolve front-panel runtime loop poll" >&2
  exit 1
fi

max_frame_bytes=0
max_frame_label=""
while read -r address label; do
  frame_bytes="$(frame_bytes_for_symbol "$address")"
  headroom_bytes=$((stack_capacity_bytes - frame_bytes))
  printf 'stack frame=%dB headroom=%dB task=%s\n' \
    "$frame_bytes" "$headroom_bytes" "$label"
  if (( frame_bytes > max_frame_bytes )); then
    max_frame_bytes="$frame_bytes"
    max_frame_label="$label"
  fi
  if (( frame_bytes >= stack_capacity_bytes )); then
    echo "task stack frame exceeds CPU0 stack: ${frame_bytes}B >= ${stack_capacity_bytes}B (${label})" >&2
    exit 1
  fi
  if (( frame_bytes > max_boot_stack_bytes || headroom_bytes < min_boot_stack_headroom_bytes )); then
    echo "task stack frame exceeds boot budget: ${frame_bytes}B, headroom=${headroom_bytes}B, task=${label}" >&2
    exit 1
  fi
done < <(
  "$nm_bin" -Sn --demangle "$elf_path" | awk '
    /TaskStorage<.*>>::poll$/ { print $1 " " substr($0, index($0, $4)) }
  '
  printf '%s\n' "$runtime_loop_symbol"
)

echo "CPU0 stack capacity=${stack_capacity_bytes}B largest_frame=${max_frame_bytes}B task=${max_frame_label} headroom=$((stack_capacity_bytes - max_frame_bytes))B minimum=${min_boot_stack_headroom_bytes}B"
