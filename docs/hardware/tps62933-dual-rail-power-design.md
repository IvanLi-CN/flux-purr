# Flux Purr TPS62933 Dual-Rail Power Design

This document freezes the current power-tree baseline for the `ESP32-S3FH4R2` revision.

## 1) Scope

- Input bus: `VSYS`, expected operating range `5 V ~ 28 V`
- MCU rail: fixed `3.3 V`, nominal load up to `1 A`
- Fan rail: adjustable `3.0 V ~ 5.0 V`, nominal fan load up to `0.5 A`
- Both rails use `TPS62933DRLR`

## 2) Frozen architecture

- `U_3V3`: `TPS62933DRLR`, fixed `3.3 V` rail for `ESP32-S3FH4R2` and low-voltage logic
- `U_FAN`: `TPS62933DRLR`, adjustable fan rail
- Both converters use the same switching-frequency and inductor baseline:
  - `RT -> GND`
  - `fSW = 1.2 MHz`
  - `L = 3.3 uH`
- Fan control is still MCU-direct:
  - `GPIO35 -> FAN_EN`
  - `GPIO36 -> FAN_VSET_PWM`

This keeps both buck stages as similar as practical while leaving only the fan rail with the extra feedback-injection network.

## 3) Why `1.2 MHz` for both rails

TI specifies a typical minimum on-time of `70 ns` and gives the frequency-foldback boundary as:

`VIN_MAX ~= VOUT / (fSW * tON_MIN)`

Using `fSW = 1.2 MHz`:

- `3.0 V` fan-min output gives `VIN_MAX ~= 35.7 V`
- `3.3 V` MCU output gives `VIN_MAX ~= 39.3 V`
- `5.0 V` fan-max output gives `VIN_MAX ~= 59.5 V`

With a real input ceiling of `28 V`, both rails stay clear of the minimum-on-time foldback boundary while still using a smaller inductor than the `500 kHz` option.

## 4) Shared passive baseline

Unless bench validation proves otherwise, both rails should start from the same passive baseline:

- `CIN = 2 x 10 uF / 50 V X7R` plus `100 nF` close to `VIN`
- `BST = 100 nF`
- `SS = 47 nF`
- `COUT = 2 x 22 uF X7R`
- Effective output capacitance should remain at or above roughly `20 uF` at operating bias
- Optional feed-forward capacitor across the upper feedback resistor:
  - start with `10 pF ~ 12 pF`
  - keep it close to the FB divider
  - final value is bench-tuning territory, not a paper-only guarantee

## 5) Common inductor selection

The common inductor target for both rails is:

- `3.3 uH`
- shielded power inductor
- `Isat >= 3 A`
- `Irms >= 1.5 A`
- `DCR <= 80 mOhm` preferred

The currently approved compact part is:

- `FTC252012S3R3MBCA`

This part fits the current design assumptions for:

- fan rail: up to `5 V / 0.5 A`
- MCU rail: up to `3.3 V / 1 A`

If the real fan startup current or the 3.3 V load budget grows materially, re-check the inductor margin before layout freeze.

## 6) Fixed `3.3 V` rail

Recommended feedback network:

- `RFBB = 10 kOhm`
- `RFBT = 31.6 kOhm`

This gives the normal `TPS62933` output relation:

`VOUT ~= 0.8 * (1 + RFBT / RFBB)`

The `3.3 V` rail should not depend on firmware-generated enables; it must be available early enough for the MCU to boot.

## 7) Adjustable fan rail

### 7.1 Target behavior

- Fan supply range: `3.0 V ~ 5.0 V`
- `GPIO35` hard-enables or disables the fan rail through the `TPS62933DRLR EN` pin
- `GPIO36` supplies PWM that is converted into a DC control voltage and injected into `FB`
- `EN` must have its own weak pulldown so the fan rail stays off during reset and while the MCU pin is high-impedance

### 7.2 Frozen control network

- `RFBB = 10 kOhm` from `FB` to `GND`
- `RFBT = 47 kOhm` from `FAN_VCC` to `FB`
- `RINJ = 75 kOhm` from `VCTRL` to `FB`
- `RPWM = 10 kOhm` from MCU PWM to `VCTRL`
- `CPWM = 1 uF` from `VCTRL` to `GND`
- `REN_PD = 100 kOhm` from `EN` to `GND`

This is intentionally a single, slow RC stage. The design goal is to make `FB` see a near-DC control value instead of a lightly filtered square wave.

### 7.3 Control law

With the network above:

- `Duty = 0%` gives approximately `5.06 V`
- `Duty = 100%` gives approximately `2.99 V`
- practical approximation:
  - `VOUT ~= 5.06 - 2.07 * Duty`
  - where `Duty` is `0.0 ~ 1.0`

Firmware should treat this as an inverse mapping: higher PWM duty means lower fan voltage.

This E24 pair is intentional because it is easier to source than the earlier E96-style values. If firmware wants to cap the nominal top end close to exactly `5.0 V`, do not use a true `0%` floor. A practical floor near `3%` duty already lands very close to `5.0 V`.

### 7.4 PWM guidance

- Recommended PWM frequency from `ESP32-S3`: `20 kHz ~ 40 kHz`
- Do not inject raw PWM directly into `FB`
- Configure the PWM pin first, then assert `EN`
- For startup reliability, drive the fan at `5 V` for `100 ms ~ 300 ms` before stepping down to the requested steady-state voltage

## 8) Fan connector protection

A diode directly across the fan connector is optional. It is not required for `TPS62933` loop stability, but it can improve connector-side robustness if the fan is off-board or frequently plugged and unplugged.

Current approved optional part:

- `DSK34`

Connection:

- cathode -> `FAN+`
- anode -> `FAN-`

This part should be treated as a cable/interface clamp, not as the main regulation or compensation element.

## 9) Layout notes

- Keep the `VIN` high-current loop and `SW` node tight on both buck stages
- Place `BST`, `CIN`, and the first output capacitor as close to the IC as possible
- Keep the `FB` divider and `RINJ` close to the `FB` pin
- Keep the `VCTRL` RC filter away from the `SW` copper and the inductor fringe field
- Route the fan connector return directly into power ground; do not force it through ADC or MCU quiet-ground paths

## 10) Validation checklist before PCB freeze

- Verify `FAN_VCC` still reaches the requested minimum under worst-case `28 V` input
- Verify fan startup at cold start and at the lowest commanded steady-state voltage
- Check audible noise at low fan speeds because `TPS62933DRLR` can enter light-load `PFM`
- Measure inductor temperature on both rails
- Confirm the real MLCC effective capacitance at operating DC bias

## References

- [TI TPS62933 datasheet](https://www.ti.com/lit/ds/symlink/tps62933.pdf)
- [TI TPS62933 product page](https://www.ti.com/product/TPS62933)
