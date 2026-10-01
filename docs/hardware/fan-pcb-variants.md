# Flux Purr Fan PCB Variants

This document freezes the sibling PCB strategy for the adjustable fan rail.

## 1) Shared contract

Both variants keep the same firmware-facing contract:

- `GPIO35 = FAN_EN`
- `GPIO36 = FAN_VSET_PWM`
- `GPIO34 = FAN_TACH`
- status fields stay unchanged:
  - `fan_enabled`
  - `fan_pwm_permille`
- the shared firmware / API contract is intentionally voltage-agnostic:
  - `fan_pwm_permille` is only a normalized actuator request
  - firmware must not infer `fan-5v` vs `fan-12v`
  - firmware must not promise a `permille -> mV` conversion

The control topology also remains shared:

- `TPS62933DRLR` adjustable buck for `FAN_VCC`
- `GPIO36 -> RPWM -> VCTRL -> RINJ -> FB`
- `GPIO35 -> FAN_EN_RAW -> RSER_EN -> FAN_EN`
- weak pulldown on the real `FAN_EN` node

## 2) Archived source and overlays

- Archived base netlist: `docs/hardware/netlists/main-controller-board.enet`
- The archived base netlist is treated as the `fan-5v` baseline.
- Variant overlays live under:
  - `docs/hardware/variants/fan-5v/`
  - `docs/hardware/variants/fan-12v/`

This repository does not currently carry the live PCB CAD source. Therefore, the authoritative in-repo deliverables are:

- the archived base netlist
- variant manifests
- variant fan-rail BOM overlays
- frozen fabrication export naming

## 3) `fan-5v` variant

- Silkscreen: `5V FAN ONLY`
- Output range: `3.0 V ~ 5.06 V`
- Frozen fan-rail network:
  - `RFBB = 10 kΩ`
  - `RFBT = 47 kΩ`
  - `RINJ = 75 kΩ`
  - `RPWM = 10 kΩ`
  - `CPWM = 1 uF`
  - `REN_PD = 100 kΩ`
  - `RSER_EN = 2.2 kΩ`
- Approximate hardware control law:
  - `VOUT ~= 5.06 - 2.07 * Duty`
- Variant manifest: `docs/hardware/variants/fan-5v/variant-manifest.json`
- Fan-rail BOM overlay: `docs/hardware/variants/fan-5v/fan-rail-bom.csv`

## 4) `fan-12v` variant

- Silkscreen: `12V FAN ONLY`
- Nominal output range: `3.45 V ~ 12.14 V`
- Physical resistor designators and E24 values:
  - `R19 (RFBB) = 13 kΩ 1%`
  - `R20 (RFBT) = 150 kΩ 1%`
  - `R18 (RINJ) = 47 kΩ 1%`
  - `R15 (RPWM) = 10 kΩ 1%`
- Output capacitor rules:
  - main `FAN_VCC` output caps: `2 x 22 uF`
  - each cap must be `>=25 V`
  - footprint must be `1206` or larger
  - add `100 nF` local decoupling close to the fan connector in the live CAD source
- Approximate hardware control law:
  - `VOUT ~= 12.14 - 8.69 * Duty`
- Variant manifest: `docs/hardware/variants/fan-12v/variant-manifest.json`
- Fan-rail BOM overlay: `docs/hardware/variants/fan-12v/fan-rail-bom.csv`

## 5) Frozen fabrication naming

The generation environment must not reuse the same export names between variants.

Recommended names:

### `fan-5v`

- `flux-purr-main-controller-fan-5v.bom.csv`
- `flux-purr-main-controller-fan-5v.pos.csv`
- `flux-purr-main-controller-fan-5v.pnp.csv`
- `flux-purr-main-controller-fan-5v.gerbers.zip`

### `fan-12v`

- `flux-purr-main-controller-fan-12v.bom.csv`
- `flux-purr-main-controller-fan-12v.pos.csv`
- `flux-purr-main-controller-fan-12v.pnp.csv`
- `flux-purr-main-controller-fan-12v.gerbers.zip`

## 6) Implementation notes

- `fan_pwm_permille` is normalized across both variants; firmware and HTTP payloads must not infer or expose actual fan voltage from it.
- Future closed-loop fan control is expected to operate on temperature error and normalized actuator commands, not on a fixed fan-voltage target.
- The selected `fan-12v` range covers the tested fan's approximately `3.3 V ~ 3.5 V` low-speed operating region; assembled-board startup and tolerance behavior still require bench validation, and any later startup tuning stays outside the shared firmware contract.
- Because the live CAD source is not checked into this repository, the `100 nF` local decoupling requirement for `fan-12v` is frozen as a manifest / layout action rather than a checked-in designator.
