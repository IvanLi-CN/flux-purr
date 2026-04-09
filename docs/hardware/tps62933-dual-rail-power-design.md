# Flux Purr TPS62933 Dual-Rail Power Design

This document freezes the current power-tree baseline for the `ESP32-S3FH4R2` revision.

The archived controller-board netlist in this repository remains the `fan-5v` reference implementation. The sibling `fan-12v` PCB is frozen through `docs/hardware/fan-pcb-variants.md` and its variant manifests.

## 1) Scope

- Board input bus: `VBUS`, expected operating range `5 V ~ 28 V`
- MCU rail: fixed `3.3 V`, nominal load up to `1 A`
- Fan rail: adjustable sibling PCB variants built on the same `TPS62933DRLR` topology
  - `fan-5v`: `3.0 V ~ 5.0 V`
  - `fan-12v`: `6.6 V ~ 12.0 V`
- Both rails use `TPS62933DRLR`

## 2) Frozen architecture

- USB-C connector raw power net: `VBUS_RAW`
- Protected board power net after the fuse: `VBUS`
- Input protection chain:
  - `VBUS_RAW -> FUSE_VBUS -> VBUS`
  - `TVS_VBUS` from `VBUS` to `GND`
- `U_3V3`: `TPS62933DRLR`, fixed `3.3 V` rail for `ESP32-S3FH4R2` and low-voltage logic
- `U_FAN`: `TPS62933DRLR`, adjustable fan rail
- Both converters use the same switching-frequency and inductor baseline:
  - `RT -> GND`
  - `fSW = 1.2 MHz`
  - `L = 3.3 uH`
- Fan control is still MCU-direct:
  - `GPIO35 -> FAN_EN_RAW -> 2.2 kOhm -> FAN_EN`
  - `GPIO36 -> FAN_VSET_PWM`

This keeps both buck stages as similar as practical while leaving only the fan rail with the extra feedback-injection network.

This fuse-plus-TVS structure is reasonable for the current fault model:

- the fuse is the sacrificial element for downstream shorts, including accidental shorts involving exposed heater hardware
- the TVS clamps the protected board-side bus rather than the raw connector pin

Do not over-interpret the TVS. On a `28 V` PD-capable bus, TVS standoff and clamp margins are tight. It is useful for transient cleanup and fault-energy shunting, but it is not a substitute for a dedicated high-voltage surge or overvoltage management stage.

## 3) Why `1.2 MHz` for both rails

TI specifies a typical minimum on-time of `70 ns` and gives the frequency-foldback boundary as:

`VIN_MAX ~= VOUT / (fSW * tON_MIN)`

Using `fSW = 1.2 MHz`:

- `3.0 V` fan-min output gives `VIN_MAX ~= 35.7 V`
- `3.3 V` MCU output gives `VIN_MAX ~= 39.3 V`
- `5.0 V` fan-5v max output gives `VIN_MAX ~= 59.5 V`
- `12.0 V` fan-12v max output gives `VIN_MAX ~= 142.9 V`

With a real input ceiling of `28 V`, both fan variants and the fixed MCU rail stay clear of the minimum-on-time foldback boundary while still using a smaller inductor than the `500 kHz` option.

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

Variant-specific capacitor rules:

- archived `fan-5v` base netlist keeps its current lower-voltage output-cap footprint set
- `fan-12v` must upgrade every capacitor directly tied to `FAN_VCC` to `>=25 V`
- the two main `fan-12v` output caps must use `1206` or larger footprints
- `fan-12v` must add a local `100 nF` decoupling capacitor close to the fan connector in the live CAD source

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

- fan rail: up to `5 V / 0.5 A` on `fan-5v`
- fan rail: planned `12 V` class operation on `fan-12v`, still pending bench confirmation for thermal and startup margin
- MCU rail: up to `3.3 V / 1 A`

If the real fan startup current or the 3.3 V load budget grows materially, re-check the inductor margin before layout freeze.

## 6) Fixed `3.3 V` rail

Recommended feedback network:

- `RFBB = 10 kOhm`
- `RFBT = 31.6 kOhm`
- `CFF = 12 pF`

This gives the normal `TPS62933` output relation:

`VOUT ~= 0.8 * (1 + RFBT / RFBB)`

The `3.3 V` rail should not depend on firmware-generated enables; it must be available early enough for the MCU to boot.

Implemented UVLO network:

- `RUVLO_TOP = 220 kOhm` from `VBUS` to `VSYS_OK`
- `RUVLO_BOT = 68 kOhm` from `VSYS_OK` to `GND`
- `VSYS_OK -> EN`

Using the `TPS62933` `EN` thresholds and bias currents, this implemented network gives approximately:

- rising enable near `4.97 V`
- falling disable near `4.49 V`

This matches the board intent of treating anything below about `4.5 V` as undervoltage while only re-enabling once the source is back near `5 V`.

## 7) Adjustable fan rail variants

### 7.1 Shared behavior

- `GPIO35` hard-enables or disables the fan rail through the `TPS62933DRLR EN` path
- `GPIO36` supplies PWM that is converted into a DC control voltage and injected into `FB`
- `EN` must have its own weak pulldown so the fan rail stays off during reset and while the MCU pin is high-impedance
- the implemented board inserts a `2.2 kOhm` series resistor between the MCU-side `FAN_EN_RAW` net and the actual `FAN_EN` node at the TPS pin
- one channel of the shared `PESD3V3S2UT` is placed on `FAN_EN_RAW`; the second channel is used by `RTD_ADC`
- the archived base netlist at `docs/hardware/netlists/main-controller-board.enet` is the `fan-5v` baseline
- all PCB-variant overlays are frozen under `docs/hardware/variants/`

### 7.2 Shared control network

Both PCB variants keep the same control topology:

- `RFBB = 10 kOhm` from `FB` to `GND`
- `RINJ = 75 kOhm` from `VCTRL` to `FB`
- `RPWM = 10 kOhm` from MCU PWM to `VCTRL`
- `CPWM = 1 uF` from `VCTRL` to `GND`
- `REN_PD = 100 kOhm` from `EN` to `GND`
- `RSER_EN = 2.2 kOhm` from `FAN_EN_RAW` to `FAN_EN`
- optional `CFF = 12 pF` across the upper FB resistor

This is intentionally a single, slow RC stage. The design goal is to make `FB` see a near-DC control value instead of a lightly filtered square wave.

### 7.3 `fan-5v` frozen variant

- `RFBT = 47 kOhm`
- `Duty = 0%` gives approximately `5.06 V`
- `Duty = 100%` gives approximately `2.99 V`
- practical approximation:
  - `VOUT ~= 5.06 - 2.07 * Duty`
  - where `Duty` is `0.0 ~ 1.0`
- silkscreen requirement: `5V FAN ONLY`

### 7.4 `fan-12v` frozen variant

- `RFBT = 124 kOhm 1%`
- `Duty = 0%` gives approximately `12.04 V`
- `Duty = 100%` gives approximately `6.59 V`
- practical approximation:
  - `VOUT ~= 12.04 - 5.46 * Duty`
  - where `Duty` is `0.0 ~ 1.0`
- all capacitors directly tied to `FAN_VCC` must be `>=25 V`
- the two main output capacitors must each be `22 uF` and use `1206` or larger footprints
- add `100 nF` local decoupling close to the fan connector in the live CAD source
- silkscreen requirement: `12V FAN ONLY`
- startup rule: assert `EN`, hold the rail near `12 V` for `200 ms`, then step down to the requested steady-state target

The `fan-12v` minimum steady-state point is intentionally about `6.6 V`. Reliable startup below that point is out of scope for this rail profile.

### 7.5 PWM guidance

- Recommended PWM frequency from `ESP32-S3`: `20 kHz ~ 40 kHz`
- Do not inject raw PWM directly into `FB`
- Configure the PWM pin first, then assert `EN`
- For startup reliability:
  - `fan-5v`: drive the fan near `5 V` for `100 ms ~ 300 ms` before stepping down to the requested steady-state voltage
  - `fan-12v`: drive the fan near `12 V` for `200 ms` before stepping down to the requested steady-state voltage

Firmware should treat both variants as inverse mappings: higher PWM duty means lower fan voltage.

## 8) Fan connector protection

A diode directly across the fan connector is optional. It is not required for `TPS62933` loop stability, but it can improve connector-side robustness if the fan is off-board or frequently plugged and unplugged.

Current approved optional part:

- `DSK34`

Connection:

- cathode -> `FAN+`
- anode -> `FAN-`

This part should be treated as a cable/interface clamp, not as the main regulation or compensation element. The same `DSK34` class remains acceptable for `fan-12v` because it already exceeds the required `>=40 V` Schottky class.

## 9) Layout notes

- Keep the `VIN` high-current loop and `SW` node tight on both buck stages
- Place `BST`, `CIN`, and the first output capacitor as close to the IC as possible
- Keep the `FB` divider and `RINJ` close to the `FB` pin
- Keep the `VCTRL` RC filter away from the `SW` copper and the inductor fringe field
- Route the fan connector return directly into power ground; do not force it through ADC or MCU quiet-ground paths

Component placement priorities for the fan rail:

- `BST` capacitor must sit very close to `BST` and `SW`
- `CIN` MLCCs must sit close to `VIN` and `GND` of the `TPS62933DRLR`
- the inductor should sit close to `SW`
- the first output capacitor should sit close to the inductor output and power ground return
- `RFBB`, `RFBT`, and the optional feed-forward capacitor should be grouped tightly around the `FB` pin
- `RINJ` should sit with the `FB` divider cluster, not back near the MCU
- `CPWM` should sit near `RINJ` and the `FB` cluster so `VCTRL` is a short local analog node
- `RPWM` should also prefer the `VCTRL/FB` side, leaving the long trace on the digital PWM side rather than on the analog control side
- the `EN` pulldown should sit close to the `EN` pin
- if `RSER_EN` is populated, it should sit closer to the `EN` pin than to the MCU so the protected node stays local to the buck stage
- `fan-12v` should reserve the local `100 nF` connector-side decoupling capacitor as a distinct placement task in the live CAD source

## 10) Validation checklist before PCB freeze

- Verify `FAN_VCC` still reaches the requested minimum under worst-case `28 V` input
- Verify fan startup at cold start and at the lowest commanded steady-state voltage
- Check audible noise at low fan speeds because `TPS62933DRLR` can enter light-load `PFM`
- Measure inductor temperature on both rails
- Confirm the real MLCC effective capacitance at operating DC bias
- For `fan-12v`, explicitly validate output-cap voltage rating, package size, and startup current margin before production release

## References

- [TI TPS62933 datasheet](https://www.ti.com/lit/ds/symlink/tps62933.pdf)
- [TI TPS62933 product page](https://www.ti.com/product/TPS62933)
- [Fan PCB variants](./fan-pcb-variants.md)
