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

- GPIO profile is locked to C3 front-panel baseline (`15/15` GPIO budget, no spare).
- CH224Q uses I2C dynamic request mode with `0x22` primary and `0x23` fallback address.
- CH442E route control uses `IN + EN#`; firmware init drives default route to `MCU`.
