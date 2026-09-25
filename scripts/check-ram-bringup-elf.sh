#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
elf="${1:-${repo_root}/firmware/target/xtensa-esp32s3-none-elf/release/flux-purr-ram-bringup}"

if [[ ! -f "${elf}" ]]; then
  printf 'RAM Bring-up ELF not found: %s\nBuild it with:\n  cargo +esp build -p flux-purr-ram-bringup --target xtensa-esp32s3-none-elf --target-dir firmware/target --release\n' "${elf}" >&2
  exit 2
fi

if command -v xtensa-esp32s3-elf-readelf >/dev/null 2>&1; then
  readelf_bin="xtensa-esp32s3-elf-readelf"
elif command -v readelf >/dev/null 2>&1; then
  readelf_bin="readelf"
else
  printf 'No readelf implementation is available to inspect %s\n' "${elf}" >&2
  exit 2
fi

# These are the only address windows accepted by the RAM loader.  They match
# ram-memory.x and intentionally exclude the ESP32-S3 flash mappings.
iram_start=$((16#40378000))
iram_end=$((16#403E0000))
dram_start=$((16#3FC88000))
dram_end=$((16#3FD00000))
rtc_fast_start=$((16#600FE000))
rtc_fast_end=$((16#60100000))
rtc_slow_start=$((16#50000000))
rtc_slow_end=$((16#50002000))

load_count=0
iram_bytes=0
dram_bytes=0
bad_segment=''
entry="$(${readelf_bin} -hW "${elf}" | awk '/Entry point address:/ { print $4; exit }')"
if [[ -z "${entry}" || "${entry}" != 0x* ]]; then
  printf 'ELF entry point is missing or not an address: %s\n' "${entry}" >&2
  exit 1
fi
entry_dec=$((16#${entry#0x}))

in_internal_range() {
  local address="$1"
  local end="$2"
  ((address >= iram_start && end <= iram_end)) ||
    ((address >= dram_start && end <= dram_end)) ||
    ((address >= rtc_fast_start && end <= rtc_fast_end)) ||
    ((address >= rtc_slow_start && end <= rtc_slow_end))
}

if ! in_internal_range "${entry_dec}" "${entry_dec}"; then
  printf 'ELF entry point is outside internal RAM: %s\n' "${entry}" >&2
  exit 1
fi

while read -r _type _offset virt phys _filesz memsz _flags _align; do
  [[ "${_type}" == "LOAD" ]] || continue
  load_count=$((load_count + 1))
  virt_dec=$((16#${virt#0x}))
  phys_dec=$((16#${phys#0x}))
  mem_dec=$((16#${memsz#0x}))
  virt_end_dec=$((virt_dec + mem_dec))
  phys_end_dec=$((phys_dec + mem_dec))

  if ((virt_dec != phys_dec)) || ! in_internal_range "${virt_dec}" "${virt_end_dec}" ||
    ! in_internal_range "${phys_dec}" "${phys_end_dec}"; then
    bad_segment="virt=${virt},phys=${phys},size=${memsz}"
    break
  fi

  if ((virt_dec >= iram_start && virt_end_dec <= iram_end)); then
    iram_bytes=$((iram_bytes + mem_dec))
  elif ((virt_dec >= dram_start && virt_end_dec <= dram_end)); then
    dram_bytes=$((dram_bytes + mem_dec))
  fi
done < <("${readelf_bin}" -lW "${elf}" | awk '$1 == "LOAD" { print $1, $2, $3, $4, $5, $6, $7, $8 }')

if ((load_count == 0)); then
  printf 'ELF has no PT_LOAD segments: %s\n' "${elf}" >&2
  exit 1
fi
if [[ -n "${bad_segment}" ]]; then
  printf 'ELF contains a non-internal PT_LOAD segment: %s\n' "${bad_segment}" >&2
  exit 1
fi

# espflash 4.5.0 uploads allocatable PROGBITS/INIT_ARRAY sections rather than
# PT_LOAD ranges. Validate the same section set so an orphan section cannot
# bypass the runtime RAM-image guard.
bad_section=''
while read -r _name section_type address offset size flags; do
  [[ "${section_type}" == "PROGBITS" || "${section_type}" == "INIT_ARRAY" ]] || continue
  [[ "${offset}" != "00000000" && "${offset}" != "0x00000000" ]] || continue
  [[ "${address}" != "00000000" && "${address}" != "0x00000000" ]] || continue
  [[ "${size}" != "00000000" && "${size}" != "0x00000000" ]] || continue
  [[ -n "${flags}" ]] || continue
  address_dec=$((16#${address#0x}))
  size_dec=$((16#${size#0x}))
  end_dec=$((address_dec + size_dec))
  if ! in_internal_range "${address_dec}" "${end_dec}"; then
    bad_section="name=${_name},addr=${address},size=${size}"
    break
  fi
done < <("${readelf_bin}" -SW "${elf}" | awk '
  # readelf prints single-digit section indexes as "[ 1]" and double-digit
  # indexes as "[10]"; normalize both forms before selecting the columns.
  $1 == "[" { name=$3; type=$4; address=$5; offset=$6; size=$7; flags=$9 }
  $1 ~ /^\[[0-9]+\]$/ { name=$2; type=$3; address=$4; offset=$5; size=$6; flags=$8 }
  (type == "PROGBITS" || type == "INIT_ARRAY") { print name, type, address, offset, size, flags }
')

if [[ -n "${bad_section}" ]]; then
  printf 'ELF contains a non-internal espflash loadable section: %s\n' "${bad_section}" >&2
  exit 1
fi

# Keep a little headroom for linker alignment while still making the budget
# explicit and reviewable in CI.
if ((iram_bytes > 0x5D400)); then
  printf 'IRAM PT_LOAD budget exceeded: 0x%x bytes\n' "${iram_bytes}" >&2
  exit 1
fi
if ((dram_bytes > 0x78000)); then
  printf 'DRAM PT_LOAD budget exceeded: 0x%x bytes\n' "${dram_bytes}" >&2
  exit 1
fi

if command -v espflash >/dev/null 2>&1; then
  version_output="$(espflash --version 2>&1 || true)"
  grep -q '^espflash 4\.5\.0$' <<<"${version_output}" || {
    printf 'Installed espflash must be exactly 4.5.0 for the pinned RAM-load contract: %s\n' "${version_output}" >&2
    exit 1
  }
  help_output="$(espflash flash --help 2>&1 || true)"
  grep -q -- '--ram' <<<"${help_output}" || {
    printf 'Installed espflash does not advertise --ram; refusing RAM artifact acceptance\n' >&2
    exit 1
  }
  grep -q -- '--no-stub' <<<"${help_output}" || {
    printf 'Installed espflash does not advertise --no-stub; refusing RAM artifact acceptance\n' >&2
    exit 1
  }
else
  printf 'espflash is required to verify the pinned RAM-load command; install espflash 4.5.0 before accepting %s\n' "${elf}" >&2
  exit 2
fi

printf 'RAM Bring-up ELF accepted: %s (PT_LOAD=%d, IRAM=0x%x, DRAM=0x%x)\n' \
  "${elf}" "${load_count}" "${iram_bytes}" "${dram_bytes}"
