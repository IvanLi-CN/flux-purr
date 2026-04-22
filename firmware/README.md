# Firmware

## Target profile

- Default architecture intent: `ESP32-S3FH4R2`
- Current bring-up board profile: `S3 frontpanel GC9D01 display baseline`
- Runtime style:
  - host preview: shared scene renderer + framebuffer dump + PNG conversion
  - device runtime: `Embassy + esp-hal-embassy + SPI2.into_async() + five-way input mock runtime`

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
  - after a short settle, enter the interactive front-panel runtime
  - default build (`esp32s3`) enters the mock-only app dashboard/menu flow
  - diagnostic build (`esp32s3,frontpanel-key-test`) enters `Key Test` for GPIO mapping calibration

## Shared scene rendering

- Shared module: `firmware/src/display/mod.rs`
- Startup scene includes:
  - corner markers (`TL/TR/BL/BR` colors)
  - top `UP` direction marker
  - RGB bar
  - grayscale bar
  - bottom panel label text
- Front-panel runtime preview includes:
  - key-test idle / short / double / long
  - dashboard / dashboard manual
  - menu / preset temp / active cooling / WiFi info / device info

## Fan / heater output contract

- Shared GPIO contract:
  - `HEATER_PWM = GPIO47`
  - `FAN_EN = GPIO35`
  - `FAN_PWM = GPIO36`
  - `RGB_B_PWM = GPIO37`
  - `RGB_G_PWM = GPIO38`
  - `RGB_R_PWM = GPIO39`
  - `FAN_TACH = GPIO34` (reserved only in this round)
- Shared PWM frequency target remains `25 kHz`, but the current front-panel runtime keeps heater and fan outputs in `safe-off` while UI actions stay mock-only.
- The Web and firmware reducers may toggle `heaterEnabled` / `fanEnabled` state for display, but those flags do not drive the physical output stage in this phase.
- Historical `fan-cycle` smoke-test behavior remains documented in `#8tesd`; it is no longer the active runtime contract for the default `esp32s3-fan-cycle` artifact on this branch.

## Build commands

- Before any Xtensa build in a fresh terminal:
  - `source /Users/ivan/export-esp.sh`

- Host tests:
  - `cargo test --manifest-path firmware/Cargo.toml`
- Host lint:
  - `cargo clippy --manifest-path firmware/Cargo.toml --all-targets --all-features -- -D warnings`
- Host release build:
  - `cargo build --manifest-path firmware/Cargo.toml --release`
- Xtensa app runtime build:
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin esp32s3-fan-cycle --release`
- Xtensa key-test calibration build:
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3,frontpanel-key-test --bin esp32s3-fan-cycle --release`

## Host preview workflow

- Render a front-panel runtime framebuffer:
  - `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard docs/specs/fk3u7-frontpanel-input-interaction/assets/dashboard.framebuffer.bin`
- The preview tool writes two framebuffer artifacts:
  - logical preview framebuffer: `<preset>.framebuffer.bin` (`RGB565 LE`, `160x50`) for owner-facing PNG generation
  - panel-order companion: `<preset>.panel.framebuffer.bin` (`RGB565 BE`, `50x160`) after applying the same GC9D01 orientation transform used on-device
- Convert the logical preview framebuffer to PNG:
  - `python3 /Users/ivan/.codex/skills/firmware-display-preview/scripts/fb_to_png.py --format rgb565 --endian le --width 160 --height 50 --in docs/specs/fk3u7-frontpanel-input-interaction/assets/dashboard.framebuffer.bin --out docs/specs/fk3u7-frontpanel-input-interaction/assets/dashboard.png`
- Preview assets land under:
  - `docs/specs/fk3u7-frontpanel-input-interaction/assets/`

## MCU agentd flow

- Repo-local config: `mcu-agentd.toml`
- MCU id: `esp32s3_frontpanel`
- Configured ELF artifact:
  - `firmware/target/xtensa-esp32s3-none-elf/release/esp32s3-fan-cycle`
- Typical flow:
  - `source /Users/ivan/export-esp.sh`
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3,frontpanel-key-test --bin esp32s3-fan-cycle --release`
  - `mcu-agentd --non-interactive config validate`
  - `mcu-agentd --non-interactive selector get esp32s3_frontpanel`
  - if selector is missing, `mcu-agentd --non-interactive selector list esp32s3_frontpanel`
  - after the intended selector is confirmed, `mcu-agentd --non-interactive flash esp32s3_frontpanel`
  - `mcu-agentd --non-interactive monitor esp32s3_frontpanel --reset`
  - 校准完成后，重新构建默认 `esp32s3` 版本并重复 flash/monitor 验证 App mock runtime

## Hardware baseline notes

- GPIO profile is locked to the S3 front-panel baseline (`24` firmware-active GPIO, center key on `GPIO0`).
- LCD `DC/MOSI/SCLK/BLK` intentionally mirrors the `mains-aegis` S3 cluster on `GPIO10/11/12/13`.
- LCD reset and chip-select are locked to `GPIO14/15` for the current front-panel wiring.
- `GPIO47` (chip pin `37`) is the heater-control PWM output for a low-side `BUK9Y14-40B,115` MOSFET stage.
- `GPIO48` (chip pin `36`) is reserved as the buzzer PWM / tone output.
- The board uses two `TPS62933DRLR` stages from the main input bus: one fixed `3.3 V` rail and one adjustable fan rail whose exact voltage behavior depends on the PCB variant and is not modeled in shared firmware.
- `GPIO39/38/37` are frozen as the `RGB_R/G/B` PWM outputs for the discrete status LED, with `GPIO39` reusing the package `MTCK` signal under the default USB-JTAG configuration.
- The fixed `3.3 V` rail uses an external UVLO divider on `VSYS_OK` (`220 kOhm` to `VBUS`, `68 kOhm` to `GND`) and enables at about `4.97 V` rising / `4.49 V` falling.
- FAN enable is owned by MCU `GPIO35`, but the implemented board routes it as `FAN_EN_RAW -> 2.2 kOhm -> FAN_EN` with the weak pulldown on the actual `EN` node; `GPIO36` provides the normalized fan-actuator PWM that is filtered and injected into the fan rail `FB` node.
- `GPIO34` is wired to `FAN_TACH` in hardware, but it is not yet part of the current firmware board-profile active GPIO set.
- Front-panel center key is directly wired to `GPIO0`, using the standard active-low BOOT-button pattern.
- LCD backlight is owned by MCU `GPIO13`, but at the system level it is active-low because the front-panel board routes `BLK` into a high-side PMOS gate.
- `GPIO35/36/34` 仍保留现有风扇硬件连线，但当前前面板 runtime 在本阶段保持 safe-off，不把 mock UI 状态接到真实风扇执行链路。

## Notes

- The repository-root `.cargo/config.toml` carries the `build-std` and `linkall.x` settings required for `--manifest-path firmware/Cargo.toml` invocations from the repo root.
- `firmware/build.rs` adds `defmt.x` for Xtensa builds, and `mcu-agentd.toml` stays pinned to `espflash` + `defmt` decoding.
- Host checks keep using the std preview path so repository checks can run without Xtensa hardware.
- This round does not implement touch input, heater power, RTD sensing, tach feedback, or production UI logic.
