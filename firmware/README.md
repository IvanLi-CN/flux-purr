# Firmware

## Target profile

- Default architecture intent: `ESP32-S3`
- Runtime style: `no_std` + async polling primitives (`embassy-futures`)

## Build commands

- Local host sanity build:
  - `cargo build --manifest-path firmware/Cargo.toml`
- ESP32-S3 build (with Xtensa toolchain installed):
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --release`

## Future C3 switch

If hardware selection changes to ESP32-C3, keep API types stable and switch target to
`riscv32imc-unknown-none-elf` in CI/release scripts.
