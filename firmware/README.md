# Firmware

## Target profile

- Default architecture intent: `ESP32-S3FH4R2`
- Runtime style: `no_std` + async polling primitives (`embassy-futures`)

## Build commands

- Local host sanity build:
  - `cargo build --manifest-path firmware/Cargo.toml`
- ESP32-S3 build:
  - `cargo +esp build --manifest-path firmware/Cargo.toml --target xtensa-esp32s3-none-elf --release`

## Hardware baseline notes

- GPIO profile is locked to the S3 front-panel baseline (`20` active GPIO, no GPIO expander, center key on `GPIO0` BOOT).
- CH224Q uses I2C dynamic request mode with `0x22` primary and `0x23` fallback address.
- Main input voltage sense uses `GPIO1` with a `56 kOhm / 5.1 kOhm` divider (`28 V -> ~2.34 V` at the ADC pin).
- `GPIO2` / `ADC1_CH1` is reserved for a `PT1000` RTD input using a `2.49 kOhm` precision bias resistor, `100 Ohm` series resistor, and `100 nF` ADC shunt capacitor.
- LCD `DC/MOSI/SCLK/BLK` intentionally mirrors the `mains-aegis` S3 cluster on `GPIO10/11/12/13`.
- `GPIO47` (chip pin `37`) is the heater-control PWM output for a low-side `BUK9Y14-40B,115` MOSFET stage.
- The board uses two `TPS62933DRLR` stages from the main input bus: one fixed `3.3 V` rail and one adjustable fan rail.
- FAN enable is directly controlled by MCU `GPIO35` with an external weak pulldown on `EN`; `GPIO36` provides the fan-voltage setpoint PWM that is filtered and injected into the fan rail `FB` node.
- Front-panel center key is directly wired to `GPIO0`, using the standard active-low BOOT-button pattern.
- LCD backlight PWM is directly driven by MCU `GPIO13`.
