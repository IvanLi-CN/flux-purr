# Firmware

## Target profile

- Default architecture intent: `ESP32-S3FH4R2`
- Current bring-up board profile: `S3 frontpanel GC9D01 display baseline`
- Runtime style:
  - host preview: shared scene renderer + framebuffer dump + PNG conversion
  - device bring-up: `Embassy + esp-hal-embassy + SPI2.into_async()`

## GC9D01 display bring-up baseline

- Driver: [`IvanLi-CN/gc9d01-rs`](https://github.com/IvanLi-CN/gc9d01-rs) async API
- Preserved firmware artifact name: `esp32s3-fan-cycle`
- Display bus: `SPI2` async, `Mode0`, `10 MHz`
- Locked panel profile:
  - `panel_160x50`
  - `width = 160`
  - `height = 50`
  - `orientation = Landscape`
  - `dx = 15`
  - `dy = 0`
- Locked LCD pins:
  - `DC = GPIO10`
  - `MOSI = GPIO11`
  - `SCLK = GPIO12`
  - `BLK = GPIO13`
  - `RES = GPIO14`
  - `CS = GPIO15`
- Current bring-up flow:
  - boot -> startup calibration screen
  - play demo sequence once
  - return to startup calibration screen and hold
- Final post-verification goal:
  - switch to startup screen only and keep it resident until a later firmware replaces it

## Shared scene rendering

- Shared module: `firmware/src/display/mod.rs`
- Startup scene includes:
  - corner markers (`TL/TR/BL/BR` colors)
  - top `UP` direction marker
  - RGB bar
  - grayscale bar
  - bottom panel label text
- Demo sequence includes:
  - solid red / green / blue
  - wide / fine checker
  - shapes / lines / text / triangles / grid

## Build commands

- Before any Xtensa build in a fresh terminal:
  - `source /Users/ivan/export-esp.sh`

- Host tests:
  - `cargo test --manifest-path firmware/Cargo.toml`
- Host lint:
  - `cargo clippy --manifest-path firmware/Cargo.toml --all-targets --all-features -- -D warnings`
- Host release build:
  - `cargo build --manifest-path firmware/Cargo.toml --release`
- Xtensa display bring-up build:
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin esp32s3-fan-cycle --release`

## Host preview workflow

- Render the startup scene framebuffer:
  - `cargo run --manifest-path firmware/Cargo.toml --bin display_preview -- startup docs/specs/vmekj-s3-gc9d01-display-bringup/assets/startup.framebuffer.bin`
- Convert `RGB565 LE` framebuffer to PNG:
  - `python3 /Users/ivan/.codex/skills/firmware-display-preview/scripts/fb_to_png.py --format rgb565 --endian le --width 160 --height 50 --in docs/specs/vmekj-s3-gc9d01-display-bringup/assets/startup.framebuffer.bin --out docs/specs/vmekj-s3-gc9d01-display-bringup/assets/startup.preview.png`
- Preview assets land under:
  - `docs/specs/vmekj-s3-gc9d01-display-bringup/assets/`

## MCU agentd flow

- Repo-local config: `mcu-agentd.toml`
- MCU id: `esp32s3_frontpanel`
- Configured ELF artifact:
  - `firmware/target/xtensa-esp32s3-none-elf/release/esp32s3-fan-cycle`
- Typical flow:
  - `source /Users/ivan/export-esp.sh`
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin esp32s3-fan-cycle --release`
  - `mcu-agentd --non-interactive config validate`
  - `mcu-agentd --non-interactive selector get esp32s3_frontpanel`
  - if selector is missing, `mcu-agentd --non-interactive selector list esp32s3_frontpanel`
  - after the intended selector is confirmed, `mcu-agentd --non-interactive flash esp32s3_frontpanel`
  - `mcu-agentd --non-interactive monitor esp32s3_frontpanel --reset`

## Hardware baseline notes

- GPIO profile is locked to the S3 front-panel baseline (`21` firmware-active GPIO, center key on `GPIO0`).
- LCD `DC/MOSI/SCLK/BLK` intentionally mirrors the `mains-aegis` S3 cluster on `GPIO10/11/12/13`.
- LCD reset and chip-select are locked to `GPIO14/15` for the current front-panel wiring.
- `GPIO47` remains the heater-control PWM output.
- `GPIO48` remains reserved as the buzzer PWM / tone output.
- `GPIO35/36/34` stay wired for the existing fan stage, but this firmware round does not drive the fan path.

## Notes

- The repository-root `.cargo/config.toml` carries the `build-std` and `linkall.x` settings required for `--manifest-path firmware/Cargo.toml` invocations from the repo root.
- `firmware/build.rs` adds `defmt.x` for Xtensa builds, and `mcu-agentd.toml` stays pinned to `espflash` + `defmt` decoding.
- Host checks keep using the std preview path so repository checks can run without Xtensa hardware.
- This round does not implement touch input, heater power, RTD sensing, tach feedback, or production UI logic.
