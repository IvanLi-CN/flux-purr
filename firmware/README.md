# Firmware

## Target profile

- Default architecture intent: `ESP32-S3FH4R2`
- Current bring-up board profile: `S3 frontpanel fan baseline`
- Runtime style:
  - host checks: `no_std` library + host stub binary
  - device bring-up: `esp-hal` LEDC runtime for `xtensa-esp32s3-none-elf`

## Fan bring-up baseline

- `FAN_EN`: `GPIO35`
- `FAN_PWM`: `GPIO36`
- `FAN_TACH`: `GPIO34` (reserved only in this round)
- PWM frequency target: `25 kHz`
- Cycle order: `10s high -> 10s low -> 10s mid -> 10s stop -> repeat`
- Frozen duty points:
  - `high = 30‰` (about `5.0 V`)
  - `mid = 300‰` (about `4.4 V`)
  - `low = 500‰` (about `4.0 V`)
  - `stop = EN low`
- Control law note: fan control is **inverse PWM-to-voltage** through the `TPS62933` feedback path; higher duty means lower target fan voltage.

## Build commands

- Before any Xtensa build in a fresh terminal:
  - `source /Users/ivan/export-esp.sh`

- Local host sanity build:
  - `cargo build --manifest-path firmware/Cargo.toml`
- Local host tests:
  - `cargo test --manifest-path firmware/Cargo.toml`
- ESP32-S3 fan bring-up build (with Xtensa toolchain installed):
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin esp32s3-fan-cycle --release`

## Hardware baseline notes

- GPIO profile is locked to the S3 front-panel baseline (`21` firmware-active GPIO, no GPIO expander, center key on `GPIO0` BOOT).
- CH224Q uses I2C dynamic request mode with `0x22` primary and `0x23` fallback address on a shared I2C bus that also hosts one `M24C64` EEPROM and `4.7 kOhm` pullups to `3V3`.
- Main input voltage sense uses `GPIO1` with a `56 kOhm / 5.1 kOhm` divider (`28 V -> ~2.34 V` at the ADC pin).
- `GPIO2` / `ADC1_CH1` is reserved for a `PT1000` RTD input using a `2.49 kOhm` precision bias resistor, `2.2 kOhm` ADC-side series resistor, `100 nF` ADC shunt capacitor, and a low-leakage ESD clamp near the MCU.
- LCD `DC/MOSI/SCLK/BLK` intentionally mirrors the `mains-aegis` S3 cluster on `GPIO10/11/12/13`.
- `GPIO47` (chip pin `37`) is the heater-control PWM output for a low-side `BUK9Y14-40B,115` MOSFET stage.
- `GPIO48` (chip pin `36`) is reserved as the buzzer PWM / tone output.
- The board uses two `TPS62933DRLR` stages from the main input bus: one fixed `3.3 V` rail and one adjustable fan rail.
- The fixed `3.3 V` rail uses an external UVLO divider on `VSYS_OK` (`220 kOhm` to `VBUS`, `68 kOhm` to `GND`) and enables at about `4.97 V` rising / `4.49 V` falling.
- FAN enable is owned by MCU `GPIO35`, but the implemented board routes it as `FAN_EN_RAW -> 2.2 kOhm -> FAN_EN` with the weak pulldown on the actual `EN` node; `GPIO36` provides the fan-voltage setpoint PWM that is filtered and injected into the fan rail `FB` node.
- `GPIO34` is wired to `FAN_TACH` in hardware, but it is not yet part of the current firmware board-profile active GPIO set.
- Front-panel center key is directly wired to `GPIO0`, using the standard active-low BOOT-button pattern.
- LCD backlight PWM is directly driven by MCU `GPIO13`.

## MCU agentd flow

- Repo-local config: `/Users/ivan/.codex/worktrees/80d2/flux-purr/mcu-agentd.toml`
- MCU id: `esp32s3_frontpanel`
- The configured ELF artifact is:
  - `firmware/target/xtensa-esp32s3-none-elf/release/esp32s3-fan-cycle`
- Typical first-use sequence:
  - `source /Users/ivan/export-esp.sh`
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin esp32s3-fan-cycle --release`
  - `mcu-agentd config check`
  - `mcu-agentd selector list esp32s3_frontpanel`
  - `mcu-agentd selector set esp32s3_frontpanel <serial-port>`
  - `mcu-agentd flash esp32s3_frontpanel`
  - `mcu-agentd monitor esp32s3_frontpanel --reset`

## Notes

- The repository-root `.cargo/config.toml` carries the `build-std` and `linkall.x` settings required for `--manifest-path firmware/Cargo.toml` invocations from the repo root.
- The `firmware/build.rs` linker hook adds `defmt.x` for Xtensa builds, and `mcu-agentd.toml` is pinned to `espflash` + `defmt` decoding.
- Host `clippy/build` keeps using the stub binary path so repository checks can run without Xtensa-specific dependencies.
- The current round does not implement tach feedback, stall recovery, heater power, RTD sensing, or LCD/output drivers.
