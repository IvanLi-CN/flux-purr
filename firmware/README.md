# Firmware

## Target profile

- Default architecture intent: `ESP32-S3FH4R2`
- Current bring-up board profile: `S3 frontpanel GC9D01 display baseline`
- Runtime style:
  - host preview: shared scene renderer + framebuffer dump + PNG conversion
  - device bring-up: `Embassy + esp-hal-embassy + SPI2.into_async() + MCPWM fan control`

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
- Front-panel board backlight polarity:
  - `BLK` is active-low on the panel board
  - `Q5` (`BSS84AKW`) switches `3V3 -> LEDA` on the high side
  - `R55 100 kOhm` pulls `BLK` up to `3V3`, so firmware must drive low or use inverted PWM for visible light
- Current startup behavior:
  - boot -> startup calibration screen
  - keep the startup screen resident until a later firmware replaces it

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

## Fan control contract

- Shared GPIO contract:
  - `FAN_EN = GPIO35`
  - `FAN_PWM = GPIO36`
  - `FAN_TACH = GPIO34` (reserved only in this round)
- Shared PWM frequency target: `25 kHz`
- `fan_pwm_permille` is a normalized actuator command owned by firmware.
- The shared firmware contract is intentionally voltage-agnostic:
  - firmware does not model the real `FAN_VCC`
  - firmware does not distinguish `fan-5v` vs `fan-12v`
  - firmware does not infer millivolts from `fan_pwm_permille`
- Actual rail range, silkscreen limits, capacitor rules, and board-specific tuning remain in hardware docs.
- Future fan control is expected to close the loop on temperature / thermal error, not on inferred fan voltage.

## Fan bring-up baseline

- Cycle order: `10s high -> 10s low -> 10s mid -> 10s stop -> repeat`
- Frozen duty points for smoke tests and bench bring-up:
  - `high = 30‰`
  - `mid = 300‰`
  - `low = 500‰`
  - `stop = EN low`
- These points are normalized actuator setpoints only. They are not promises about the actual fan voltage on any PCB variant.

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
  - `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin display_preview -- startup docs/specs/vmekj-s3-gc9d01-display-bringup/assets/startup.framebuffer.bin`
- The preview tool writes two framebuffer artifacts:
  - logical preview framebuffer: `startup.framebuffer.bin` (`RGB565 LE`, `160x50`) for owner-facing PNG generation
  - panel-order companion: `startup.panel.framebuffer.bin` (`RGB565 BE`, `50x160`) after applying the same GC9D01 orientation transform used on-device
- Convert the logical preview framebuffer to PNG:
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
- `GPIO47` (chip pin `37`) is the heater-control PWM output for a low-side `BUK9Y14-40B,115` MOSFET stage.
- `GPIO48` (chip pin `36`) is reserved as the buzzer PWM / tone output.
- The board uses two `TPS62933DRLR` stages from the main input bus: one fixed `3.3 V` rail and one adjustable fan rail whose exact voltage behavior depends on the PCB variant and is not modeled in shared firmware.
- The fixed `3.3 V` rail uses an external UVLO divider on `VSYS_OK` (`220 kOhm` to `VBUS`, `68 kOhm` to `GND`) and enables at about `4.97 V` rising / `4.49 V` falling.
- FAN enable is owned by MCU `GPIO35`, but the implemented board routes it as `FAN_EN_RAW -> 2.2 kOhm -> FAN_EN` with the weak pulldown on the actual `EN` node; `GPIO36` provides the normalized fan-actuator PWM that is filtered and injected into the fan rail `FB` node.
- `GPIO34` is wired to `FAN_TACH` in hardware, but it is not yet part of the current firmware board-profile active GPIO set.
- Front-panel center key is directly wired to `GPIO0`, using the standard active-low BOOT-button pattern.
- LCD backlight is owned by MCU `GPIO13`, but at the system level it is active-low because the front-panel board routes `BLK` into a high-side PMOS gate.
- `GPIO35/36/34` stay wired for the existing fan stage, and the display test firmware continues to run the frozen fan-cycle behavior alongside the static startup screen.

## Notes

- The repository-root `.cargo/config.toml` carries the `build-std` and `linkall.x` settings required for `--manifest-path firmware/Cargo.toml` invocations from the repo root.
- `firmware/build.rs` adds `defmt.x` for Xtensa builds, and `mcu-agentd.toml` stays pinned to `espflash` + `defmt` decoding.
- Host checks keep using the std preview path so repository checks can run without Xtensa hardware.
- This round does not implement touch input, heater power, RTD sensing, tach feedback, or production UI logic.
