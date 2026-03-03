# Flux Purr C3 Front-Panel Hardware Baseline

This document freezes the hardware integration baseline for the ESP32-C3 revision.

## 1) SoC and major chips

- MCU: `ESP32-C3-FH4`
- PD sink: `CH224Q` (I2C dynamic voltage request)
- USB2.0 routing: `CH442E` (`IN + EN#` control)
- 5 V rail: `TPS62933`
- 3.3 V rail: `RT9013-33GB`
- Fan regulator: `RT9043GB` (`PWM + EN` control)
- Front panel expander: `TCA6408A @ 0x21`
- Display: same 1.12-inch panel class used in `iso-usb-hub`

## 2) Locked GPIO allocation (15/15)

| Function | GPIO | Notes |
| --- | ---: | --- |
| USB D- | 18 | Native USB pins |
| USB D+ | 19 | Native USB pins |
| CH442E IN | 8 | Route select |
| CH442E EN# | 9 | Active low enable |
| I2C SDA | 4 | Shared by CH224Q + TCA6408A |
| I2C SCL | 5 | Shared by CH224Q + TCA6408A |
| Front-panel INT# | 2 | From TCA6408A, active low/open-drain |
| LCD SCLK | 21 | SPI clock |
| LCD MOSI | 20 | SPI MOSI |
| LCD DC | 7 | Data/command |
| LCD BLK | 6 | Backlight (PWM allowed) |
| FAN PWM | 3 | RT9043GB control injection path |
| FAN EN | 1 | Fan rail enable |
| HEATER PWM | 10 | Main heating PWM |
| TEMP ADC | 0 | Temperature sensing input |

GPIO budget is intentionally full: `15/15`, no spare line.

## 3) Front-panel TCA6408A map

`TCA6408A @ 0x21` pins:

- `P0`: Center key
- `P1`: Right key
- `P2`: Down key
- `P3`: Left key
- `P4`: Up key
- `P5`: LCD RES
- `P6`: LCD CS
- `P7`: Reserved

## 4) CH224Q control baseline

- Use I2C dynamic mode with 7-bit address `0x22` (fallback compatible `0x23`).
- Support requests for `5/9/12/15/20/28 V`.
- Keep PD state visible in firmware status model (`request` vs `contract` voltage).

## 5) CH442E routing baseline

- `EN#` is treated as active low.
- Route behavior:
  - `EN# = 1` => disabled/high-Z routing state
  - `EN# = 0` and `IN = 0` => `MCU` path
  - `EN# = 0` and `IN = 1` => `SINK` path
- Boot strategy: enter safe state first, then initialize to default `MCU` route.

## 6) Power tree (frozen)

```text
USB-C PD input
  -> CH224Q negotiates source
  -> main high-voltage bus (up to 28V request)
  -> TPS62933 buck to 5V
  -> RT9013-33GB LDO to 3V3
  -> RT9043GB adjustable fan rail (PWM + EN)
```

## 7) ESP32-C3 strapping and bring-up constraints

Use strapping pins with care during reset window:

- `GPIO2`, `GPIO8`, `GPIO9` are strapping-related on ESP32-C3.
- Ensure external pull network and peripheral defaults do not force unwanted boot mode.
- Keep `CH442E` control network compatible with boot requirements before firmware config.

Reference:

- ESP32-C3 datasheet: <https://documentation.espressif.com/esp32-c3_datasheet_en.html>

## 8) Known trade-offs

- `fan_tach` is intentionally not connected in this revision.
- No spare GPIO remains for add-ons; any new peripheral requires reallocation or an extra expander.
