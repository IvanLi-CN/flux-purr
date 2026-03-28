# Flux Purr C3 Front-Panel Hardware Baseline

This document freezes the hardware integration baseline for the ESP32-C3 revision.

## 1) SoC and major chips

- MCU: `ESP32-C3-FH4`
- PD sink: `CH224Q` (I2C dynamic voltage request)
- 5 V rail: `TPS62933`
- 3.3 V rail: `RT9013-33GB`
- Fan regulator: `RT9043GB` (`PWM + EN` control)
- Front panel expander: `TCA6408A @ 0x21`
- Display: same 1.12-inch panel class used in `iso-usb-hub`

## 2) Active MCU GPIO allocation (14/15)

| Function | GPIO | Notes |
| --- | ---: | --- |
| USB D- | 18 | Native USB pins |
| USB D+ | 19 | Native USB pins |
| I2C SDA | 4 | Shared by CH224Q + TCA6408A |
| I2C SCL | 5 | Shared by CH224Q + TCA6408A |
| Front-panel INT# | 2 | From TCA6408A, active low/open-drain |
| LCD SCLK | 21 | SPI clock |
| LCD MOSI | 20 | SPI MOSI |
| LCD DC | 7 | Data/command |
| FAN EN | 6 | Direct MCU control |
| FAN PWM | 3 | RT9043GB control injection path |
| VIN ADC | 1 | `ADC1_CH1`, main input voltage sense |
| Center Key / BOOT | 9 | Direct MCU boot strap input, active low button to GND |
| HEATER PWM | 10 | Main heating PWM |
| TEMP ADC | 0 | Temperature sensing input |

Reserved but intentionally unused MCU pins:

- `GPIO8`

`GPIO8` stays reserved as a BOOT-assist strapping pin and should be held high-compatible during reset.

## 3) Front-panel TCA6408A map

`TCA6408A @ 0x21` pins:

- `P0`: Reserved
- `P1`: Right key
- `P2`: Down key
- `P3`: Left key
- `P4`: Up key
- `P5`: LCD RES
- `P6`: LCD CS
- `P7`: LCD BLK (on/off only)

## 4) CH224Q control baseline

- Use I2C dynamic mode with 7-bit address `0x22` (fallback compatible `0x23`).
- Support requests for `5/9/12/15/20/28 V`.
- Keep PD state visible in firmware status model (`request` vs `contract` voltage).

## 5) VIN sense baseline

- ADC pin: `GPIO1` / `ADC1_CH1`
- Nominal divider:
  - `R_HIGH = 56 kOhm` from `VIN` to `GPIO1`
  - `R_LOW = 5.1 kOhm` from `GPIO1` to `GND`
- Divider ratio: `Vadc = Vin * 5.1 / (56 + 5.1) ~= Vin / 11.98`
- At `VIN = 28 V`, `GPIO1` sees about `2.34 V`, leaving margin for ESP32-C3 ADC operation with high attenuation enabled.
- Recommendation: use `1%` resistors and add `100 nF` from `GPIO1` to `GND` near the MCU to stabilize the sampled node.

## 6) Center key / BOOT baseline

- Front-panel center key is directly wired to MCU `GPIO9`.
- This key doubles as the ROM boot-mode key: hold the center key during reset to request download mode.
- Hardware implementation should follow the usual active-low boot button pattern: released = pulled high, pressed = short to `GND`.
- To keep download mode reachable, reserve `GPIO8` and add a weak pull-up so `GPIO9` low can be interpreted as a bootloader request.

## 7) FAN enable baseline

- FAN regulator enable is directly driven by MCU `GPIO6`.
- `GPIO6` is no longer shared with LCD backlight, so `FAN_EN` can stay out of the ESP32-C3 boot-strap path.
- Recommended default: add a weak pulldown such as `100 kOhm` on `FAN_EN`, keeping the fan rail disabled until firmware configures the pin.

## 8) Power tree (frozen)

```text
USB-C PD input
  -> CH224Q negotiates source
  -> main high-voltage bus (up to 28V request)
  -> 56k / 5.1k divider to GPIO1 VIN sense
  -> TPS62933 buck to 5V
  -> RT9013-33GB LDO to 3V3
  -> RT9043GB adjustable fan rail (GPIO3 PWM + GPIO6 EN)
```

## 9) ESP32-C3 strapping and bring-up constraints

Use strapping pins with care during reset window:

- `GPIO2`, `GPIO8`, `GPIO9` are strapping-related on ESP32-C3.
- Ensure external pull network and peripheral defaults do not force unwanted boot mode.
- `GPIO9` is now the dedicated center-key / BOOT input, so keep its released state high and route the key as an active-low switch to `GND`.
- `GPIO8` stays reserved and should remain high-compatible during reset so the `GPIO9` BOOT key path can request download mode reliably.

Reference:

- ESP32-C3 datasheet: <https://documentation.espressif.com/esp32-c3_datasheet_en.html>

## 10) Known trade-offs

- `fan_tach` is intentionally not connected in this revision.
- `LCD BLK` moves to `TCA6408A P7`, so hardware backlight control is GPIO-expander based rather than native MCU PWM.
- Only `GPIO8` remains unused, and it is reserved for boot-strap compatibility rather than general expansion.
- VIN sense accuracy depends on ADC calibration, divider tolerance, and input ripple.
