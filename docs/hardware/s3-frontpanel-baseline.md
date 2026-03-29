# Flux Purr S3 Front-Panel Hardware Baseline

This document freezes the hardware integration baseline for the ESP32-S3FH4R2 revision.

## 1) SoC and major chips

- MCU: `ESP32-S3FH4R2`
- PD sink: `CH224Q` (I2C dynamic voltage request)
- 5 V rail: `TPS62933`
- 3.3 V rail: `RT9013-33GB`
- Fan regulator: `RT9043GB` (`PWM + EN` control)
- Display: same 1.12-inch panel class used in `iso-usb-hub`
- Front-panel keys: direct-to-MCU, no I2C GPIO expander

## 2) Direct MCU GPIO allocation (20 active)

| Function | GPIO | Notes |
| --- | ---: | --- |
| Center Key / BOOT | 0 | Active low button to `GND`, ROM boot strap |
| VIN ADC | 1 | `ADC1_CH0`, main input voltage sense |
| RTD ADC | 2 | `ADC1_CH1`, reserved for `PT1000` sensing |
| HEATER PWM | 5 | Main heating PWM |
| I2C SDA | 8 | CH224Q only |
| I2C SCL | 9 | CH224Q only |
| LCD DC | 10 | Matches `mains-aegis` LCD control cluster |
| LCD MOSI | 11 | SPI MOSI |
| LCD SCLK | 12 | SPI clock |
| LCD BLK | 13 | Direct MCU PWM, aligned with `mains-aegis` |
| LCD RES | 14 | Direct reset output |
| LCD CS | 15 | Direct chip-select output |
| Right Key | 16 | Direct GPIO input |
| Down Key | 17 | Direct GPIO input |
| Left Key | 18 | Direct GPIO input |
| USB D- | 19 | Native USB pins |
| USB D+ | 20 | Native USB pins |
| Up Key | 21 | Direct GPIO input |
| FAN EN | 35 | Direct MCU enable, matches `mains-aegis` fan block |
| FAN PWM | 36 | RT9043 GB control injection path |

Available headroom remains on other ESP32-S3 GPIOs. This baseline intentionally mirrors the `mains-aegis` `GPIO10/11/12/13` LCD cluster plus `GPIO35/36` fan control pair while still avoiding `GPIO3`, `GPIO45`, `GPIO46`, and the flash/PSRAM GPIO block.

## 3) CH224Q control baseline

- Use I2C dynamic mode with 7-bit address `0x22` (fallback compatible `0x23`).
- Support requests for `5/9/12/15/20/28 V`.
- Keep PD state visible in firmware status model (`request` vs `contract` voltage).

## 4) VIN sense baseline

- ADC pin: `GPIO1` / `ADC1_CH0`
- Nominal divider:
  - `R_HIGH = 56 kOhm` from `VIN` to `GPIO1`
  - `R_LOW = 5.1 kOhm` from `GPIO1` to `GND`
- Divider ratio: `Vadc = Vin * 5.1 / (56 + 5.1) ~= Vin / 11.98`
- At `VIN = 28 V`, `GPIO1` sees about `2.34 V`, leaving margin for ESP32-S3 ADC operation with high attenuation enabled.
- Recommendation: use `1%` resistors and add `100 nF` from `GPIO1` to `GND` near the MCU to stabilize the sampled node.

## 5) RTD sense baseline

- Sensor type baseline: `PT1000`
- ADC pin: `GPIO2` / `ADC1_CH1`
- Recommended direct-to-ADC network for `ESP32-S3`:
  - `3V3 -> R_REF = 2.49 kOhm (0.1%, <= 25 ppm/C) -> RTD_SENSE`
  - `PT1000 -> RTD_SENSE to GND`
  - `RTD_SENSE -> 100 Ohm -> GPIO2`
  - `GPIO2 -> 100 nF -> GND` placed close to the MCU
- This network keeps the ADC source impedance low, follows Espressif's common "ADC pin with external capacitor" practice, and gives a useful voltage span for `PT1000` hotplate temperatures:
  - about `0.95 V` at `0 C`
  - about `1.18 V` at `100 C`
  - about `1.52 V` at `300 C`
  - about `1.70 V` at `450 C`
- Firmware recommendation: use ADC calibration and prefer `ADC_ATTEN_DB_6` when the expected RTD range stays within about `0 ~ 360 C` for this exact `2.49 kOhm + PT1000` network.
- If the product requirement is really `0 ~ 500 C` while still trying to stay inside the better `ADC_ATTEN_DB_6` range, increase `R_REF` to about `3.0 kOhm` and re-freeze the divider math before layout.
- If the actual probe turns out to be `PT100` instead of `PT1000`, do not keep this direct-divider topology. `PT100` should move to a dedicated RTD front-end (`MAX31865` class, or precision current source + amplifier), because lead resistance and ADC span both become too weak for a direct MCU ADC solution.
- If the RTD is wired off-board, reserve an optional small capacitor footprint (`1 nF` max) directly across the probe for EMI cleanup.

## 6) Center key / BOOT baseline

- Front-panel center key is directly wired to MCU `GPIO0`.
- This key doubles as the ROM boot-mode key: hold the center key during reset to request download mode.
- Hardware implementation should follow the standard active-low boot button pattern: released = pulled high, pressed = short to `GND`.
- `GPIO46` must remain low or floating during reset for download mode compatibility; this baseline leaves it unused.

## 7) LCD and fan control baseline

- `LCD DC/MOSI/SCLK/BLK` are placed on `GPIO10/11/12/13`, matching the frozen LCD cluster used by `mains-aegis`.
- `LCD BLK` is directly driven by MCU `GPIO13` and must support PWM dimming.
- `FAN_EN` is directly driven by MCU `GPIO35`; add a weak pulldown such as `100 kOhm` so the fan rail stays disabled before firmware init.
- `FAN_PWM` is directly driven by MCU `GPIO36`.
- `GPIO34` is intentionally left free so a future revision can add `FAN_TACH` without breaking the fan-control block convention used by `mains-aegis`.

## 8) Power tree (frozen)

```text
USB-C PD input
  -> CH224Q negotiates source
  -> main high-voltage bus (up to 28V request)
  -> 56k / 5.1k divider to GPIO1 VIN sense
  -> PT1000 divider to GPIO2 RTD sense
  -> TPS62933 buck to 5V
  -> RT9013-33GB LDO to 3V3
  -> RT9043GB adjustable fan rail (GPIO36 PWM + GPIO35 EN)
```

## 9) ESP32-S3FH4R2 bring-up constraints

- Native USB uses `GPIO19` (`D-`) and `GPIO20` (`D+`).
- Strapping pins on ESP32-S3 are `GPIO0`, `GPIO3`, `GPIO45`, and `GPIO46`.
- Avoid using `GPIO3`, `GPIO45`, and `GPIO46` for front-panel or power-control signals.
- `GPIO26 ~ GPIO32` are generally occupied by SPI flash / PSRAM and are intentionally avoided in this baseline.

Reference:

- ESP32-S3 GPIO guide: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/gpio.html>
- ESP32-S3 boot mode selection: <https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/boot-mode-selection.html>
- ESP32-S3 USB device guide: <https://docs.espressif.com/projects/esp-idf/en/release-v5.5/esp32s3/api-reference/peripherals/usb_device.html>

## 10) Known trade-offs

- `fan_tach` is intentionally not connected in this revision; `GPIO34` is left available if that signal is added later.
- Front-panel keys are all direct GPIOs, so debounce and wake behavior are purely firmware responsibilities.
- VIN sense and RTD sense accuracy both depend on ADC calibration, resistor tolerance, and board-level noise.
