# Firmware

## Target profile

- Default architecture intent: `ESP32-S3FH4R2`
- Current bring-up board profile: `S3 frontpanel GC9D01 display baseline`
- Runtime style:
  - host preview: shared scene renderer + framebuffer dump + PNG conversion
  - device runtime: `Embassy + esp-hal-embassy + SPI2.into_async() + five-way input + PID heater runtime`

## GC9D01 display bring-up baseline

- Driver: [`IvanLi-CN/gc9d01-rs`](https://github.com/IvanLi-CN/gc9d01-rs) async API
- Main firmware artifact name: `flux-purr`
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
  - default build (`esp32s3`) enters the app runtime with real RTD/PID/fan state rendering
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
  - `FAN_TACH = GPIO34` (reserved only in this round)
- Runtime truth source:
  - `current_temp_c` is the live PT1000-derived temperature sample from `GPIO2 / ADC1`
  - `target_temp_c` is clamped to `0..=400°C`
  - `Preset Temp` defaults are `50 / 100 / 120 / 150 / 180 / 200 / 210 / 220 / 250 / 300°C`
  - `heater_enabled` is the user arm state toggled by center short-press
  - `heater_output_percent` is the live PID duty rendered in the Dashboard bottom bar
  - `fan_enabled` is the actual fan runtime state, not a mock toggle
- Heater control:
  - `GPIO47` runs formal PID PWM at `2 kHz`
  - control interval is `1 Hz`
  - RTD open/short, ADC read failure, or `temp >= 420°C` force heater fault-latch and duty `0%`
  - fault-latch requires the user to clear the fault condition and re-arm with another center short-press
- Fan control:
  - normal heating keeps the fan off
  - `temp >= 360°C` forces the fan on
  - fan stays on until temperature drops below `340°C`
  - the `Active Cooling` page is informational in the formal runtime; it documents the safety policy instead of exposing a writable fan override
  - on the current board, full-speed fan output is `GPIO35=high` plus `GPIO36 duty=0%`
- PD policy:
  - boot still requests `20 V` from `CH224Q`
  - later PD status changes are observed and logged only; they do not latch or gate heater output
- Historical `fan-cycle` smoke-test behavior remains documented in `#8tesd`; it is no longer the active runtime contract for the default `flux-purr` artifact.

## CH224Q PD request bring-up

- `GPIO8/9` host the shared I2C bus for `CH224Q` and `M24C64`.
- The app runtime programs `CH224Q` register `0x0A` on boot and requests `20 V`.
- Firmware first tries `0x22`, then falls back to `0x23`; if neither address acknowledges after retries, boot aborts before the app runtime continues.
- After boot request/settle, the runtime polls CH224Q status for observation and defmt logging only.

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
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin flux-purr --release`
- Xtensa key-test calibration build:
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3,frontpanel-key-test --bin flux-purr --release`

## Host preview workflow

- Render a front-panel runtime framebuffer:
  - `cargo run --manifest-path firmware/Cargo.toml --features host-preview --bin frontpanel_preview -- dashboard docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard.framebuffer.bin`
- The preview tool writes two framebuffer artifacts:
  - logical preview framebuffer: `<preset>.framebuffer.bin` (`RGB565 LE`, `160x50`) for owner-facing PNG generation
  - panel-order companion: `<preset>.panel.framebuffer.bin` (`RGB565 BE`, `50x160`) after applying the same GC9D01 orientation transform used on-device
- Convert the logical preview framebuffer to PNG:
  - `python3 /Users/ivan/.codex/skills/firmware-display-preview/scripts/fb_to_png.py --format rgb565 --endian le --width 160 --height 50 --in docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard.framebuffer.bin --out docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/dashboard.png`
- Preview assets land under:
  - `docs/specs/q2aw6-heater-pid-frontpanel-runtime/assets/`

## MCU agentd flow

- Repo-local config: `mcu-agentd.toml`
- MCU id: `esp32s3_frontpanel`
- Configured ELF artifact:
  - `firmware/target/xtensa-esp32s3-none-elf/release/flux-purr`
- Typical flow:
  - `source /Users/ivan/export-esp.sh`
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --features esp32s3 --bin flux-purr --release`
  - `mcu-agentd --non-interactive config validate`
  - `mcu-agentd --non-interactive selector get esp32s3_frontpanel`
  - if selector is missing, `mcu-agentd --non-interactive selector list esp32s3_frontpanel`
  - after the intended selector is confirmed, `mcu-agentd --non-interactive flash esp32s3_frontpanel`
  - `mcu-agentd --non-interactive monitor esp32s3_frontpanel --reset`
  - 板级验证 heater/fan/runtime logs 后，如需输入校准再临时构建 `frontpanel-key-test` 版本

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
- `GPIO35/36/34` 保持当前风扇硬件连线；默认运行态只在 overtemp 条件下驱动真实风扇，不再接受 mock UI 直接切换。

## Notes

- The repository-root `.cargo/config.toml` carries the `build-std` and `linkall.x` settings required for `--manifest-path firmware/Cargo.toml` invocations from the repo root.
- `firmware/build.rs` adds `defmt.x` for Xtensa builds, and `mcu-agentd.toml` stays pinned to `espflash` + `defmt` decoding.
- Host checks keep using the std preview path so repository checks can run without Xtensa hardware.
- This round still does not implement touch input, tach feedback, external PID tuning, or VIN/PD-aware power compensation.
