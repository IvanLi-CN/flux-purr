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
| TEMP ADC | 2 | `ADC1_CH1`, temperature sensing input |
| HEATER PWM | 5 | Main heating PWM |
| FAN PWM | 6 | RT9043GB control injection path |
| FAN EN | 7 | Direct MCU enable, default low |
| I2C SDA | 8 | CH224Q only |
| I2C SCL | 9 | CH224Q only |
| Right Key | 10 | Direct GPIO input |
| LCD MOSI | 11 | SPI MOSI |
| LCD SCLK | 12 | SPI clock |
| LCD DC | 13 | Data/command |
| LCD RES | 14 | Direct reset output |
| LCD CS | 15 | Direct chip-select output |
| LCD BLK | 16 | Direct MCU PWM output |
| Down Key | 17 | Direct GPIO input |
| Left Key | 18 | Direct GPIO input |
| USB D- | 19 | Native USB pins |
| USB D+ | 20 | Native USB pins |
| Up Key | 21 | Direct GPIO input |

Available headroom remains on other ESP32-S3 GPIOs. This baseline intentionally avoids `GPIO3`, `GPIO45`, `GPIO46`, and the flash/PSRAM GPIO block.

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

## 5) Center key / BOOT baseline

- Front-panel center key is directly wired to MCU `GPIO0`.
- This key doubles as the ROM boot-mode key: hold the center key during reset to request download mode.
- Hardware implementation should follow the standard active-low boot button pattern: released = pulled high, pressed = short to `GND`.
- `GPIO46` must remain low or floating during reset for download mode compatibility; this baseline leaves it unused.

## 6) LCD and fan control baseline

- `LCD BLK` is directly driven by MCU `GPIO16` and must support PWM dimming.
- `FAN_EN` is directly driven by MCU `GPIO7`; add a weak pulldown such as `100 kOhm` so the fan rail stays disabled before firmware init.
- `FAN_PWM` is directly driven by MCU `GPIO6`.

## 7) Power tree (frozen)

```text
USB-C PD input
  -> CH224Q negotiates source
  -> main high-voltage bus (up to 28V request)
  -> 56k / 5.1k divider to GPIO1 VIN sense
  -> TPS62933 buck to 5V
  -> RT9013-33GB LDO to 3V3
  -> RT9043GB adjustable fan rail (GPIO6 PWM + GPIO7 EN)
```

## 8) ESP32-S3FH4R2 bring-up constraints

- Native USB uses `GPIO19` (`D-`) and `GPIO20` (`D+`).
- Strapping pins on ESP32-S3 are `GPIO0`, `GPIO3`, `GPIO45`, and `GPIO46`.
- Avoid using `GPIO3`, `GPIO45`, and `GPIO46` for front-panel or power-control signals.
- `GPIO26 ~ GPIO32` are generally occupied by SPI flash / PSRAM and are intentionally avoided in this baseline.

Reference:

- ESP32-S3 GPIO guide: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/gpio.html>
- ESP32-S3 boot mode selection: <https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/boot-mode-selection.html>
- ESP32-S3 USB device guide: <https://docs.espressif.com/projects/esp-idf/en/release-v5.5/esp32s3/api-reference/peripherals/usb_device.html>

## 9) Known trade-offs

- `fan_tach` is intentionally not connected in this revision.
- Front-panel keys are all direct GPIOs, so debounce and wake behavior are purely firmware responsibilities.
- VIN sense accuracy depends on ADC calibration, divider tolerance, and input ripple.
