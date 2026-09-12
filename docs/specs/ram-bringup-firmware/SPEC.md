# Flux Purr RAM Bring-up Firmware

## Context and Scope

`flux-purr-ram-bringup` is a separate ESP32-S3 firmware artifact for short-lived
board validation. It is loaded into internal RAM with `espflash --ram` and does
not become a feature of the installed `flux-purr` product binary. The canonical
operator surface is the repository CLI:

- `flux-purr ram-run preview <display|frontpanel|status-light> --port <SERIAL_PORT>`
- Preview commands accept an optional `--theme light|dark`; light is the
  default and the option is ignored/rejected for hardware tests.
- `flux-purr ram-run test <buttons|adc|i2c|rgb|buzzer|fan> --port <SERIAL_PORT>`
- `flux-purr ram-run exit --port <SERIAL_PORT>`

The port is always supplied explicitly. The RAM path never scans, substitutes,
or switches to a newly enumerated serial path.

## Goals

- Render the display, front-panel, and status-light previews on the real panel.
- Exercise the five-way buttons, VIN/RTD ADC inputs, the whitelisted read-only
  I2C identity registers, RGB LED, bounded buzzer cue, and the restricted fan
  output.
- Identify the running image as `product` or `ram_bringup`, including its
  `buildId` and command capabilities.
- Keep all RAM execution and command failure paths heater-off, PD-write-free,
  and EEPROM-write-free.
- Preserve an explicit `--install` escape hatch through the existing developer
  flash safety gate. Installation replaces the sole factory app; `exit` only
  resets and verifies the installed product identity.

## Requirements

- `REQ-RAM-001`: Bring-up MUST be a separate `flux-purr-ram-bringup` ESP32-S3
  artifact whose PT_LOAD segments are limited to internal RAM and whose default
  CLI path never writes Flash.
- `REQ-RAM-002`: USB identity MUST distinguish only `product` and `ram_bringup`;
  missing, legacy, malformed, ROM, and non-responsive identity observations
  MUST remain `unknown` and MUST NOT authorize RAM-session reuse.
- `REQ-RAM-003`: Bring-up MUST expose only the closed preview/test command set,
  with heater, PD, EEPROM, arbitrary GPIO, and arbitrary I2C writes excluded.
- `REQ-RAM-004`: `ram-run` MUST probe the supplied port before reset, reuse only
  a matching build/capability, load with `espflash --ram --no-stub` otherwise,
  and route `--install` through the existing direct flash gate.
- `REQ-RAM-005`: `exit` MUST reset without Flash writes and report success only
  after a verified `product` identity.

## Non-goals

- No second app slot, partition-table change, bootloader change, eFuse change,
  or automatic product-firmware recovery.
- No arbitrary GPIO, I2C address/register/payload, heater, PD, EEPROM, LAN, or
  Web control surface.
- No dedicated desktop preview executable or preview Cargo feature.

## Protocol

USB JSONL `Identity` carries `firmwareKind`, whose closed values are `product`
and `ram_bringup`. Missing, malformed, legacy, ROM, or non-responsive identity
is treated as `unknown` by the CLI and never inferred as a product or RAM image.

Bring-up frames use `type: "ram_bringup"` and one closed command name, with an
optional closed `theme` value (`light` or `dark`) for preview commands:
`preview_display`, `preview_frontpanel`, `preview_status_light`, `test_buttons`,
`test_adc`, `test_i2c`, `test_rgb`, `test_buzzer`, or `test_fan`. Product firmware
parses the frame but returns `unsupported_frame` without executing it.

I2C only reads the compile-time whitelist (`0x22:0x01` and `0x22:0x09`). The
buzzer emits a finite cue. Fan test uses `FanPhase::Mid` at `300‰` for ten
seconds and then `FanPhase::Stop`.

## RAM image contract

The crate owns its manifest, linker script, and release artifact. PT_LOAD
segments must remain within ESP32-S3 internal IRAM, DRAM, RTC fast, or RTC slow
windows; flash-mapped `0x42000000` and `0x3C000000` addresses are rejected.
`scripts/check-ram-bringup-elf.sh` checks segment ranges, IRAM/DRAM budgets,
and that the installed `espflash` advertises both `--ram` and `--no-stub`.

## Verification

- `VER-RAM-001` (covers: REQ-RAM-002, REQ-RAM-003): Given the firmware control-plane tests, When identity and
  `ram_bringup` frames are parsed, Then product rejection, legacy unknown
  classification, and the closed command set are proven. Evidence: `cargo test
  -p flux-purr-firmware control_plane` and `cargo test -p flux-purr-firmware
  ram_bringup`.
- `VER-RAM-002` (covers: REQ-RAM-004, REQ-RAM-005): Given the CLI unit tests, When RAM preview/test/install/reload/
  exit paths are exercised, Then exact-port handling, capability matching,
  timeout bounds, and Flash gates remain explicit. Evidence: `cargo test
  -p flux-purr-devd --bin flux-purr`.
- `VER-RAM-003` (covers: REQ-RAM-001): Given the release ELF, When the RAM checker runs, Then every
  PT_LOAD is internal RAM, budgets are respected, and the pinned espflash flags
  are available. Evidence: `cargo +esp build -p flux-purr-ram-bringup --target
  xtensa-esp32s3-none-elf --target-dir firmware/target --release` and
  `scripts/check-ram-bringup-elf.sh`.
- `VER-RAM-004` (covers: REQ-RAM-001, REQ-RAM-003, REQ-RAM-004, REQ-RAM-005): Authorized hardware validation remains a separate gate requiring a single
  owner-authorized serial port; build and mock results are not hardware proof.

## Related ADRs

None
