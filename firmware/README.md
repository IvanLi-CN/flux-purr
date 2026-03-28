# Firmware

## Target profile

- Default architecture intent: `ESP32-C3`
- Runtime style: `no_std` + async polling primitives (`embassy-futures`)

## Build commands

- Local host sanity build:
  - `cargo build --manifest-path firmware/Cargo.toml`
- ESP32-C3 build:
  - `cargo build --manifest-path firmware/Cargo.toml --target riscv32imc-unknown-none-elf --release`

## Hardware baseline notes

- GPIO profile is locked to the C3 front-panel baseline (`14` active GPIO, direct `FAN_EN` on `GPIO8`, reserved `GPIO9`).
- CH224Q uses I2C dynamic request mode with `0x22` primary and `0x23` fallback address.
- Main input voltage sense uses `GPIO1` with a `56 kOhm / 5.1 kOhm` divider (`28 V -> ~2.34 V` at the ADC pin).
- FAN enable is directly controlled by MCU `GPIO8`; keep the hardware default low during reset.
