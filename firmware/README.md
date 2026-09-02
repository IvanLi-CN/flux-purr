# Firmware

## Target profile

- Default architecture intent: `ESP32-S3FH4R2`
- Current bring-up board profile: `S3 frontpanel GC9D01 display baseline`
- Runtime style:
  - host preview: shared scene renderer + framebuffer dump + PNG conversion
  - device runtime: `Embassy + esp-hal-embassy + SPI2.into_async() + five-way input + PID heater runtime + GPIO48 buzzer cues`

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
  - dashboard / dashboard manual / dashboard-fan-off / dashboard-fan-auto / dashboard-fan-run
  - dashboard-overtemp-a / dashboard-overtemp-b
  - menu / preset temp / active cooling / WiFi info / device info

## Fan / heater / buzzer output contract

- Shared GPIO contract:
  - `HEATER_PWM = GPIO47`
  - `BUZZER_PWM = GPIO48`
  - `FAN_EN = GPIO35`
  - `FAN_PWM = GPIO36`
  - `RGB_B_PWM = GPIO37`
  - `RGB_G_PWM = GPIO38`
  - `RGB_R_PWM = GPIO39`
  - `FAN_TACH = GPIO34` (reserved only in this round)
- Runtime truth source:
  - `current_temp_c` is the live PT1000-derived temperature sample from `GPIO2 / ADC1`
  - when RTD enters fault, the front panel keeps the last valid displayed temperature instead of synthesizing `0°C`
  - `target_temp_c` is clamped to `0..=400°C`
  - Dashboard and `Preset Temp` up/down short-presses adjust by `1°C`; holding up/down repeats after the `500ms` long-press threshold, first about every `120ms` and then about every `60ms`
  - `Preset Temp` defaults are `50 / 100 / 120 / 150 / 180 / 200 / 210 / 220 / 250 / 300°C`
  - `heater_enabled` is the user arm state toggled by center short-press
  - `active_cooling_enabled` is the user-facing “主动降温” policy bit toggled by center double-press
  - `heater_output_percent` is the live PID duty rendered in the Dashboard bottom bar
  - `fan_enabled` is the actual fan runtime state, not a mock toggle
- EEPROM memory:
  - `M24C64` on shared `GPIO8/9` I2C stores current v5 memory records in two `2 KiB` slots at `0x1000` and `0x1800`; the decoder accepts v1-v5, while previous `1 KiB` slots at `0x0400` / `0x0800` and legacy `512 B` slots at `0x0000` / `0x0200` remain read-only migration sources, with the highest valid sequence restored. Legacy EEPROM records are migrated in RAM and materialized as v5 on the next successful configuration commit.
  - if EEPROM access fails, the same record falls back to two slots in the dedicated `flux_cfg` 8KiB data partition declared by `firmware/partitions.csv`; each slot owns a separate `4 KiB` erase sector so a power loss during one write leaves the other record recoverable without writing into NVS-managed space. The 4 MiB layout reserves a 2 MiB factory app partition for the WiFi HTTP release, then places `flux_cfg` at `0x210000`.
  - persisted fields are `target_temp_c`, `selected_preset_slot`, `presets_c[10]`, `active_cooling_enabled`, and Wi-Fi config fields
  - record payloads are TLV encoded with CRC validation; unknown TLVs are skipped so future fields can be appended, and newly persisted thermal-profile TLVs use an explicit `TCP2` layout marker while unmarked historical layouts remain readable
  - accepted front-panel edits debounce for about `2s` before writing the next slot
  - on FUSB302B boards, each bounded EEPROM page write releases the shared I2C bus and services PD before the next page; a successful EEPROM save does not synchronously mirror to flash
  - `heater_enabled`, live temperatures, fan runtime output, fault latch, route/menu state, and buzzer reminders are never restored from EEPROM
- Heater control:
  - the control loop runs at `20 Hz` and produces a normalized `0..100%` equivalent heat-power request; profile tick based parameters retain their `1 s` reference scale
  - the controller uses model-assisted ramp/soak plus hold PI trimming: far from target it uses an approach power, inside the target-specific brake distance it ramps toward hold power, and in hold it trims around hold power with a small PI term
  - optional `ThermalControlProfile` preview is RAM-only and can tune up to 10 target points; saved profiles persist all 10 fully materialized point-local parameter sets in redundant `2 KiB` records, while historical `1 KiB` records remain readable; missing points fall back to conservative defaults, and interpolated targets use linear interpolation
  - if CH224Q power data contains a PPS APDO that covers `20 V`, firmware uses the `pps-mos` backend. When several APDOs cover `20 V`, it selects the highest current APDO, then highest maximum voltage, then lowest minimum voltage. Outside HOLD, armed `0%` keeps the MOS off and returns the request to the configured working floor through bounded `500 mV` steps; HOLD keeps its locked PPS voltage while PWM is `0%`. `1..100%` maps equivalent power into a `100 mV` aligned request up to the selected APDO maximum. `R(T)` contributes to `min(Vsource^2/R(T), Vsource*Isource)` heater-power estimation but does not lower the request ceiling. Same-APDO changes preserve the MOS gate and wait `500 ms` between requests for the RTD guard; discrete APDO/AVS/fixed-PDO/fallback changes blank the MOS for a `275 ms` transition. Fixed-PD fallback enforces its negotiated current through `GPIO47` PWM.
  - `GPIO47` uses MCPWM at `100 Hz` for PPS and fixed-PD fallback. PPS voltage provides coarse power control; at the PPS floor and during bounded down-ramp, PWM continuously extends physical output down to `0%`. Each warmup entry applies a `1000 ms` linear physical-output soft start. HOLD inherits and locks the voltage established by Approach while PWM handles PI response. Firmware does not sweep voltage downward during HOLD; only sustained full PWM below target with insufficient rise may raise PPS in bounded `500 mV` steps at least `2 s` apart. Safe-limit convergence obeys the same step bound and cannot clamp directly to the PPS floor.
  - control interval is `50 ms (20 Hz)`
  - RTD open/short or ADC read failure forces heater fault-latch and duty `0%` without buzzer attention; valid temperature or raw ADC changes are never classified as a speed/discontinuity fault
- USB JSONL control frames use an `8 KiB` shared firmware/devd limit. This accommodates a fully materialized nine-point thermal profile save or preview request; oversized frames are rejected at the transport boundary.
  - `temp >= 420°C` enters thermal runaway, forces duty `0%`, and rejects heater arm while the runaway alert remains unacknowledged; acknowledgement never bypasses the active absolute overtemperature cutoff
  - measurement fault-latch requires the fault condition to clear before a later explicit re-arm; clearing a fault never restores heater output automatically
- Fan control:
  - heater disabled + active cooling enabled: temporary cooling policy runs full speed at `>=35°C` (`GPIO36 duty=0%`, `0‰`)
  - once active cooling has the fan running and temperature drops below `35°C`, the firmware drives `GPIO36 duty=100%` (`1000‰`) for `30s`, then stops the fan
  - heater enabled: `<=100°C` keeps the fan off; `>100°C` uses minimum-voltage enable pulses only while the live heater output is non-zero; the pulse on-window is twice the cooling-disabled pulse and capped at `50%`
  - active cooling disabled: `>100°C` minimum-voltage `0.2Hz` enable pulse capped at `25%`, `>350°C` heater lock + `50%` fan, `>360°C` full speed
  - unacknowledged thermal runaway forces the existing active-cooling envelope regardless of the owner policy: `>60°C` full speed and `40~60°C` at `50%`; the forced state ends at `<40°C` or on acknowledgement, whichever comes first
  - Dashboard `fan_display_state` is `OFF / AUTO / RUN`; `fan_enabled` remains the actual runtime output
  - the `Active Cooling` page is informational in the formal runtime; owner-facing wording should call this setting “开启主动降温”, not “风扇开机”
  - on the current board, full-speed fan output is `GPIO35=high` plus `GPIO36 duty=0%`
  - on the current board, the minimum-output-voltage fan profile is `GPIO35=high` plus `GPIO36 duty=100%` (`1000‰`); the fan rail control law is inverted
- Buzzer control:
  - `GPIO48` is driven by `MCPWM0 timer2/operator2`, separate from the heater and fan PWM channels
  - boot and idle are silent
  - fixed one-shot cues cover `ui_input / heater_on / heater_off / active_cooling_on / active_cooling_off / heater_reject`
  - accepted menu navigation, child-page enter/exit, preset edits, and other non-toggle frontpanel actions submit the generic `ui_input` feedback cue to the single-output arbiter
  - buzzer attention has only two owner-facing states: active thermal runaway and thermal-runaway acknowledgement pending
  - active thermal runaway (`temp >= 420°C`) replays the protection cue every `1s`; after temperature returns below `420°C`, an unacknowledged alert replays the reminder cue every `10s`
  - front-panel input or CLI/app runtime acknowledgement clears pending attention and the forced-fan latch, but cannot silence or clear active absolute overtemperature protection
  - the arbiter selects `thermal protection > thermal attention reminder > feedback`; feedback never interrupts an active cue, repeats of pending `ui_input` coalesce, and the latest specialized feedback replaces older pending feedback
  - each cue selected for playback starts from its first note. GPIO48 retains its MCPWM phase across duty-zero silence gaps when the selected next tone uses the active carrier frequency; a different next audible frequency must reconfigure the timer so a previous frequency stage cannot continue
  - a firmware build with the non-default `buzzer-debug` feature advertises `buzzer_debug` over native USB JSONL for developer diagnostics. It accepts only fixed feedback cues and `feedback_coalesce` / `feedback_replace` scenarios through the arbiter; it is interlocked while heating, thermal fault, fault latch, or thermal attention is active, and cannot control PWM parameters, safety cues, persistence, or LAN endpoints
- PD policy:
  - default build requests `20 V` from `CH224Q`
  - optional `pd-request-12v` / `pd-request-28v` features switch the boot request to `12 V` / `28 V`
  - the app runtime reads CH224Q `0x60~0x8F` power data after the boot request; fixed `20 V` PDO alone is not enough to enable PPS heating
  - later PD status changes are observed and logged only; they do not latch heater output, but failed adjustable-voltage writes demote heater control to the fixed-PD PWM fallback
- Historical `fan-cycle` smoke-test behavior remains documented in `#8tesd`; it is no longer the active runtime contract for the default `flux-purr` artifact.

## CH224Q PD request bring-up

- `GPIO8/9` host the shared I2C bus for `CH224Q` and `M24C64`.
- The app runtime programs `CH224Q` register `0x0A` on boot and requests the feature-selected voltage (`20 V` by default, optional `12 V` / `28 V` build variants).
- The runtime then reads CH224Q `0x60~0x8F` power data. If a PPS APDO covers `20 V`, heater control can switch to `pps-mos`; competing APDOs are selected by maximum current, maximum voltage, then minimum voltage. The live `0x50` current readback constrains safe power but does not alter the advertised capability class. Otherwise it remains on `fixed-pd-pwm-fallback`.
- In `pps-mos`, CH224Q `0x53` is used for PPS voltage requests in `100 mV` units and `0x51/0x52` for AVS requests above the PPS range. AVS `25 mV` resolution is not used for first-version hold-power trimming. The first request writes the voltage register before writing `0x0A = 6` or `0x0A = 7`.
- Firmware first tries `0x22`, then falls back to `0x23`; if neither address acknowledges after retries, boot aborts before the app runtime continues.
- After boot request/settle, the runtime polls CH224Q status for observation and defmt logging only.

## Build commands

- Before any Xtensa build in a fresh terminal:
  - `source /Users/ivan/export-esp.sh`

- Host tests:
  - `cargo test --manifest-path firmware/Cargo.toml`
- Host lint:
  - `bash scripts/check-firmware-clippy.sh`
- Host release build:
  - `cargo build --manifest-path firmware/Cargo.toml --release`
- Xtensa app runtime build:
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --target-dir firmware/target --release`
  - Equivalent explicit feature form: `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --target-dir firmware/target --features esp32s3,web_serial,net_http --bin flux-purr --release`
- Xtensa app runtime build (`12 V` variant):
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --target-dir firmware/target --no-default-features --features esp32s3,web_serial,net_http,pd-request-12v --bin flux-purr --release`
- Xtensa app runtime build (`28 V` variant):
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --target-dir firmware/target --no-default-features --features esp32s3,web_serial,net_http,pd-request-28v --bin flux-purr --release`

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

## MCU agentd diagnostic flow

- Repo-local config: `mcu-agentd.toml`
- MCU id: `esp32s3_frontpanel`
- Configured ELF artifact:
  - `firmware/target/xtensa-esp32s3-none-elf/release/flux-purr`
- `mcu-agentd` remains available for selector inspection and diagnostics. It is not the data-preserving firmware installation path because direct `espflash` execution bypasses devd's `flux_cfg` migration preflight.
- Typical diagnostic flow:
  - `source /Users/ivan/export-esp.sh`
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --target-dir firmware/target --release` (default `20 V` + real control-plane transport)
  - if a different PD cap is needed, rebuild with `--no-default-features --features esp32s3,web_serial,net_http,pd-request-12v` or `--no-default-features --features esp32s3,web_serial,net_http,pd-request-28v`
  - `mcu-agentd --non-interactive config validate`
  - `mcu-agentd --non-interactive selector get esp32s3_frontpanel`
  - if selector is missing, `mcu-agentd --non-interactive selector list esp32s3_frontpanel`
  - `mcu-agentd --non-interactive monitor esp32s3_frontpanel`
  - 板级验证使用默认 app runtime；输入校准通过正常前面板交互和 USB JSONL 状态完成
- Use the repository-local `flux-purr` CLI through `flux-purr-devd` for every real firmware installation that must preserve device configuration. Direct `mcu-agentd flash`, direct `espflash`, erase-chip, or an explicit recovery workflow are outside that preservation guarantee.

## Hardware baseline notes

- GPIO profile is locked to the S3 front-panel baseline (`24` firmware-active GPIO, center key on `GPIO0`).
- LCD `DC/MOSI/SCLK/BLK` intentionally mirrors the `mains-aegis` S3 cluster on `GPIO10/11/12/13`.
- LCD reset and chip-select are locked to `GPIO14/15` for the current front-panel wiring.
- `GPIO47` (chip pin `37`) controls the low-side heater MOSFET stage through the populated `68 Ohm` gate resistor and MCPWM at `100 Hz` in both PPS and fallback modes. `BUK9Y14-40B,115` is the primary approved part and `PSMN1R4-40YLDX` is the approved pin-compatible substitute.
- `GPIO48` (chip pin `36`) is the active buzzer PWM / tone output.
- The board uses two `TPS62933DRLR` stages from the main input bus: one fixed `3.3 V` rail and one adjustable fan rail whose exact voltage behavior depends on the PCB variant and is not modeled in shared firmware.
- `GPIO39/38/37` are frozen as the `RGB_R/G/B` PWM outputs for the discrete status LED, with `GPIO39` reusing the package `MTCK` signal under the default USB-JTAG configuration.
- The archived 2026-04-22 main-board netlist keeps a second RGB footprint (`LED2`) only as DNI; the populated baseline still uses one RGB status LED with one ballast resistor per color.
- The fixed `3.3 V` rail uses an external UVLO divider on `VSYS_OK` (`220 kOhm` to `VBUS`, `68 kOhm` to `GND`) and enables at about `4.97 V` rising / `4.49 V` falling.
- FAN enable is owned by MCU `GPIO35`, but the implemented board routes it as `FAN_EN_RAW -> 2.2 kOhm -> FAN_EN` with the weak pulldown on the actual `EN` node; `GPIO36` provides the normalized fan-actuator PWM that is filtered and injected into the fan rail `FB` node.
- `GPIO34` remains reserved for `FAN_TACH`, but the 2026-04-22 main-board netlist currently leaves it unconnected, so it is not yet part of the current firmware board-profile active GPIO set.
- Front-panel center key is directly wired to `GPIO0`, using the standard active-low BOOT-button pattern.
- LCD backlight is owned by MCU `GPIO13`, but at the system level it is active-low because the front-panel board routes `BLK` into a high-side PMOS gate.
- `GPIO35/36/34` 保持当前风扇硬件连线；默认运行态只在 overtemp 条件下驱动真实风扇，不再接受 mock UI 直接切换。

## Notes

- The repository-root `.cargo/config.toml` carries the `build-std` and `linkall.x` settings required for `--manifest-path firmware/Cargo.toml` invocations from the repo root.
- The same config bounds the ESP WiFi station RX/TX pools for the low-throughput LAN control plane. If WiFi driver or LAN task startup cannot be completed, firmware publishes a network error and continues the USB JSONL recovery/control loop.
- The ESP32-S3 executor reserves an 80 KiB shared task arena for the main loop and LAN tasks. WiFi drivers plus HTTP request/response buffers and mailbox staging use static storage so they do not consume async task-frame capacity.
- The repository-root `espflash.toml` pins `firmware/partitions.csv`, so ELF flashing installs the dedicated `flux_cfg` fallback partition together with the normal NVS, PHY, and factory-app layout. `firmware/partitions.bin` is the checked-in equivalent for the supported raw-app devd path: devd writes it at `0x8000` before the app and then resets the target. Before either real-flash path changes the `flux_cfg` address, devd reads the current device layout, stages the complete record at the target address, and verifies that staged copy before it writes the app image; a failed or unsafe preflight refuses the app write.
- `firmware/build.rs` adds `defmt.x` for Xtensa builds, and `mcu-agentd.toml` stays pinned to `espflash` + `defmt` decoding.
- Host checks keep using the std preview path so repository checks can run without Xtensa hardware.
- This round still does not implement touch input, tach feedback, external PID tuning, or closed-loop VIN/current power compensation.
