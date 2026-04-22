# Flux Purr S3 Front-Panel Hardware Baseline

This document freezes the hardware integration baseline for the ESP32-S3FH4R2 revision.

## 1) SoC and major chips

- MCU: `ESP32-S3FH4R2`
- PD sink: `CH224Q` (I2C dynamic voltage request)
- 3.3 V rail: `TPS62933DRLR` (fixed `3.3 V`)
- Fan rail: `TPS62933DRLR` adjustable sibling variants
  - `fan-5v`: `3.0 V ~ 5.0 V`
  - `fan-12v`: `6.6 V ~ 12.0 V`
- Display: same 1.12-inch panel class used in `iso-usb-hub`
- Front-panel keys: direct-to-MCU, no I2C GPIO expander
- Archived controller-board netlist: `docs/hardware/netlists/main-controller-board.enet` (`fan-5v` baseline)
- Variant overlay reference: `docs/hardware/fan-pcb-variants.md`
- Archived front-panel-board netlist: `docs/hardware/netlists/front-panel-board.enet`

## 2) Direct MCU GPIO allocation (24 active)

| Function | GPIO | Notes |
| --- | ---: | --- |
| Center Key / BOOT | 0 | Active low button to `GND`, ROM boot strap |
| VIN ADC | 1 | `ADC1_CH0`, main input voltage sense |
| RTD ADC | 2 | `ADC1_CH1`, reserved for `PT1000` sensing |
| HEATER PWM | 47 | Chip pin 37, main heating PWM |
| I2C SDA | 8 | Shared by `CH224Q` and `M24C64` EEPROM |
| I2C SCL | 9 | Shared by `CH224Q` and `M24C64` EEPROM |
| LCD DC | 10 | Matches `mains-aegis` LCD control cluster |
| LCD MOSI | 11 | SPI MOSI |
| LCD SCLK | 12 | SPI clock |
| LCD BLK | 13 | Active-low BLK gate to the front-panel backlight switch |
| LCD RES | 14 | Direct reset output |
| LCD CS | 15 | Direct chip-select output |
| Right Key | 16 | Direct GPIO input |
| Down Key | 17 | Direct GPIO input |
| Left Key | 18 | Direct GPIO input |
| USB D- | 19 | Native USB pins |
| USB D+ | 20 | Native USB pins |
| Up Key | 21 | Direct GPIO input |
| FAN TACH | 34 | Hardware-wired tach input, not yet consumed by the current firmware board profile |
| FAN EN | 35 | Direct MCU enable for the fan TPS62933 stage |
| FAN PWM | 36 | PWM input for fan-voltage setpoint injection |
| RGB B PWM | 37 | Discrete blue-channel PWM for the RGB status LED |
| RGB G PWM | 38 | Discrete green-channel PWM for the RGB status LED |
| RGB R PWM | 39 | Discrete red-channel PWM for the RGB status LED (`MTCK` package signal) |
| BUZZER PWM | 48 | Chip pin 36, buzzer tone / beep output |

Available headroom remains on other ESP32-S3 GPIOs. This baseline intentionally mirrors the `mains-aegis` `GPIO10/11/12/13` LCD cluster, keeps the fan rail on `GPIO34/35/36`, and uses `GPIO37/38/39` as a contiguous RGB status-LED PWM group while still avoiding `GPIO3`, `GPIO45`, `GPIO46`, and the flash/PSRAM GPIO block.

## 3) CH224Q control baseline

- Use I2C dynamic mode with 7-bit address `0x22` (fallback compatible `0x23`).
- Support requests for `5/9/12/15/20/28 V`.
- Keep PD state visible in firmware status model (`request` vs `contract` voltage).
- The same MCU I2C bus also carries one `M24C64` EEPROM with `E0/E1/E2` strapped low.
- The shared `SDA/SCL` bus uses `4.7 kOhm` pullups to `3V3`.

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
- Recommended protected direct-to-ADC network for `ESP32-S3`:
  - `3V3 -> R_REF = 2.49 kOhm (0.1%, <= 25 ppm/C) -> RTD_SENSE`
  - `PT1000 -> RTD_SENSE to GND`
  - `RTD_SENSE -> 2.2 kOhm -> GPIO2`
  - `GPIO2 -> 100 nF -> GND` placed close to the MCU
  - `GPIO2 -> low-leakage ESD clamp to GND`, for example one channel of `PESD3V3S2UT`
- This network keeps the RTD divider simple while adding meaningful MCU-side protection for an off-board probe and still gives a useful voltage span for `PT1000` hotplate temperatures:
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
- Controller board `GPIO13` reaches the front-panel board as net `BLK` through the panel FPC.
- Front-panel board netlist evidence:
  - `FPC1 pin 7 = BLK`
  - `R55 = 100 kOhm` pulls `BLK` up to `3V3`
  - `Q5 = BSS84AKW,115` (`P-MOS`) uses `G=BLK`, `S=3V3`, `D=U44.LEDA`
  - `U44.LEDK` is tied directly to `GND`
- Therefore `LCD BLK` is **active-low** at the system level: driving `GPIO13` low turns the backlight on, while driving it high or leaving it floating turns the backlight off.
- Backlight PWM dimming must follow this active-low polarity.
- `HEATER_PWM` is directly driven by MCU `GPIO47` (chip pin `37`) and controls a low-side heater MOSFET stage.
- `BUZZER_PWM` is directly driven by MCU `GPIO48` (chip pin `36`) and is reserved for buzzer beeps or passive-buzzer tone output via PWM.
- The RGB status LED is directly driven by MCU `GPIO39/38/37` for `R/G/B` respectively, each as an independent PWM-capable output.
- Heater switching baseline:
  - use low-side `NMOS`
  - current approved part: `BUK9Y14-40B,115`
  - `R_GATE = 10 Ohm`
  - `R_GPD = 100 kOhm`
  - PWM start point `1 kHz ~ 2 kHz`
- `FAN_EN` is directly driven by MCU `GPIO35`; add a weak pulldown such as `100 kOhm` so the fan rail stays disabled before firmware init.
- In the implemented netlist, `GPIO35` first drives `FAN_EN_RAW`, then passes through a `2.2 kOhm` series resistor into the TPS62933 `EN` pin. The weak `100 kOhm` pulldown remains on the actual `FAN_EN` node.
- `FAN_PWM` is directly driven by MCU `GPIO36`, but it is not used as a raw fan-wire PWM. It feeds the `TPS62933DRLR` fan-rail FB injection network.
- `FAN_TACH` is wired to `GPIO34` in hardware. The current firmware board profile still leaves this input outside the frozen 21-pin active set until tach support is implemented.
- One channel of the dual `PESD3V3S2UT` clamp is populated on `FAN_EN_RAW`; the other channel protects `RTD_ADC`.
- Keep the buzzer silent by default at boot. If the buzzer stage can sound when its input floats, add an external weak pulldown or use a driver topology whose default state is silent.
- Fan rail baseline:
  - use `TPS62933DRLR`
  - `RT -> GND` (`1.2 MHz`)
  - `L = 3.3 uH`
  - shared contract remains `GPIO35/36/34` plus `fan_enabled` / `fan_pwm_permille`
  - archived `fan-5v` baseline:
    - output range `3.0 V ~ 5.0 V`
    - `RFBB = 10 kOhm`
    - `RFBT = 47 kOhm`
    - `RINJ = 75 kOhm`
    - `RPWM = 10 kOhm`
    - `CPWM = 1 uF`
    - no `VCTRL` pulldown
    - `EN` uses a weak pulldown such as `100 kOhm`
    - connector silkscreen must read `5V FAN ONLY`
  - `fan-12v` overlay:
    - output range `6.6 V ~ 12.0 V`
    - keep the same network except `RFBT = 124 kOhm 1%`
    - every capacitor directly on `FAN_VCC` must be `>=25 V`
    - the two main output capacitors must use `1206` or larger footprints
    - add local `100 nF` decoupling near the fan connector in live CAD
    - connector silkscreen must read `12V FAN ONLY`
  - shared firmware contract remains voltage-agnostic; any board-specific startup tuning stays outside `fan_enabled` / `fan_pwm_permille` semantics
- See `docs/hardware/fan-pcb-variants.md` for the machine-readable variant manifests and fabrication-output naming freeze.

## 8) Power tree (frozen)

```text
USB-C PD input
  -> CH224Q negotiates source
  -> USB connector raw power net `VBUS_RAW`
  -> one-time SMD fuse to protected board bus `VBUS`
  -> `TVS_VBUS` from `VBUS` to `GND`
  -> main high-voltage board bus (up to 28V request)
  -> 56k / 5.1k divider to GPIO1 VIN sense
  -> PT1000 divider to GPIO2 RTD sense
  -> heater element switched by low-side NMOS from GPIO47 PWM
  -> TPS62933 buck to fixed 3V3
  -> TPS62933 buck to adjustable fan rail (`fan-5v` archived base or `fan-12v` sibling variant)
```

Power-stage details are frozen in:

- `docs/hardware/tps62933-dual-rail-power-design.md`
- `docs/hardware/heater-power-switch-design.md`

## 9) ESP32-S3FH4R2 bring-up constraints

- Native USB uses `GPIO19` (`D-`) and `GPIO20` (`D+`).
- Strapping pins on ESP32-S3 are `GPIO0`, `GPIO3`, `GPIO45`, and `GPIO46`.
- Avoid using `GPIO3`, `GPIO45`, and `GPIO46` for front-panel or power-control signals.
- `GPIO39` is reused here for `RGB_R_PWM`; this is acceptable as long as the design keeps the default built-in USB Serial/JTAG path and does not burn eFuses to move JTAG onto `GPIO39~42`.
- `GPIO26 ~ GPIO32` are generally occupied by SPI flash / PSRAM and are intentionally avoided in this baseline.

Reference:

- ESP32-S3 GPIO guide: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32s3/api-reference/peripherals/gpio.html>
- ESP32-S3 boot mode selection: <https://docs.espressif.com/projects/esptool/en/latest/esp32s3/advanced-topics/boot-mode-selection.html>
- ESP32-S3 USB device guide: <https://docs.espressif.com/projects/esp-idf/en/release-v5.5/esp32s3/api-reference/peripherals/usb_device.html>

## 10) Known trade-offs

- `fan_tach` is wired in hardware on `GPIO34`, but the current firmware board profile does not consume it yet.
- Front-panel keys are all direct GPIOs, so debounce and wake behavior are purely firmware responsibilities.
- VIN sense and RTD sense accuracy both depend on ADC calibration, resistor tolerance, and board-level noise.
- Heater-power control depends on direct `3.3 V` MCU gate drive, so MOSFET temperature and drain overshoot still require bench validation.
- Fan-voltage control depends on a filtered PWM-to-FB injection path, so final startup behavior, output-cap selection, and low-speed acoustics still require bench validation.
