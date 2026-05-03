# Flux Purr Heater Plate Family

This document defines the shared rules for Flux Purr heater plates. Each heater-plate version keeps its resistance target, power tables, Gerber package, and parsed geometry in its own file.

## 1) Scope

- Board type: single-sided aluminum-core PCB heater
- Copper weight: `1 oz`
- Heater load type: copper trace resistive heater
- Primary use case: small single-sided PCB reflow work
- Contact method: copper posts press directly onto the lower two circular contact pads
- Controller-side switching baseline: [heater-power-switch-design.md](heater-power-switch-design.md)

## 2) Thermal Targets

The heater plate family is optimized for soldering and reflow work around these solder systems:

- Low-temperature solder: `138 C`
- SnPb eutectic solder: `183 C`
- SAC-class lead-free solder: `217 C`

Operating limits:

- Practical normal maximum: about `250 C`
- Firmware temperature cap: `300 C`
- Forced protection threshold: `360 C`

Rows above `250 C` in the fixed-PD tables are included for cap and protection behavior, not as normal operating targets.

## 3) Supported Versions

Firmware must treat the heater plate as a calibrated load profile. The board revision, measured cold resistance, and selected firmware profile must match during bring-up.

| Profile ID | Board size | Nominal `R20` | Trace width | Routed length | Version file |
| --- | ---: | ---: | ---: | ---: | --- |
| `heater-5p6-3p2` | `56 mm x 56 mm` | `3.2 ohm` | `0.40 mm` | `2570.05 mm` | [heater-5p6-3p2.md](heater-plates/heater-5p6-3p2.md) |
| `heater-5p6-4p5` | `56 mm x 56 mm` | `4.5 ohm` | `0.30 mm` | `2742.89 mm` | [heater-5p6-4p5.md](heater-plates/heater-5p6-4p5.md) |

The `heater-5p6-3p2` and `heater-5p6-4p5` versions are peer heater-plate options. Firmware must select the matching calibrated profile for the installed plate.

## 4) Common Electrical Model

Each heater plate must be measured during bring-up and the measured cold resistance must be used by firmware power limiting.

The firmware voltage request limit is:

```text
V_cmd <= min(V_source_max, I_source_max * R_estimated(T))
```

Copper temperature coefficient used for first-order estimation:

```text
R(T) = R20 * (1 + 0.00393 * (T - 20))
```

The fixed-PD tables in the version files are intended for firmware voltage-step selection. Temperature rows are rounded and biased toward the points where each fixed voltage crosses about `3 A` or `5 A`, before applying source current limits.

## 5) Surface Finish and Solder Mask

Common solder mask rules:

- Black solder mask on the circuit side
- Heater traces covered by solder mask
- Circular pads opened in the checked packages
- Only the lower circular pads are heater supply pressure contacts

The checked packages use the lower two circular pads as the heater supply contacts. The upper circular pads and side pads are isolated copper features and must not be used as the heater supply path.

Preferred contact-pad finish:

- `ENIG` for the lowest-maintenance pressure-contact surface

Low-cost DIY contact-pad options:

- `OSP`
- Bare copper

For low-cost builds, treat OSP as a temporary shipping and handling protection for the copper surface. The contact pads remain maintenance items after repeated heat cycles.

`HASL` is not selected for the pressure-contact pads because tin can soften, creep, oxidize, or reflow in the intended operating temperature range.

## 6) Contact Mechanics

The copper posts must provide a stable low-resistance pressure contact:

- Use flat, clean copper-post contact faces.
- Maintain preload with a spring, belleville washer, spring contact, or another elastic structure.
- Avoid relying only on a one-time rigid screw preload.
- Measure contact voltage drop at operating current after assembly.
- Repeat the voltage-drop measurement after thermal cycling.

Contact voltage drop is part of the heater power path and must be treated as a bring-up measurement, not as a fixed design constant.

## 7) Manufacturing Notes

Order notes should include the selected profile ID and package path:

```text
Single-sided aluminum-core PCB heater, 1 oz copper, black solder mask on the circuit side.
Heater traces are covered by solder mask. Circular pads are exposed in the checked Gerber package.
Only the lower circular pads are heater supply pressure-contact pads; upper exposed circular pads are isolated no-connect features.
Confirm the selected profile ID and target cold heater resistance after fabrication.
Intended heater operation is up to about 250 C, with firmware cap at 300 C and forced protection at 360 C.
Confirm solder mask and dielectric suitability for repeated thermal cycling in this use case.
```
