# Flux Purr Heater Plate Design

This document defines the heater-plate baseline used with the Flux Purr heater power-switch stage.

## 1) Scope

- Board type: single-sided aluminum-core PCB heater
- Copper weight: `1 oz`
- Heater load type: copper trace resistive heater
- Primary use case: small single-sided PCB reflow work
- Contact method: copper posts press directly onto the lower two circular contact pads
- Controller-side switching baseline: [heater-power-switch-design.md](heater-power-switch-design.md)

## 2) Thermal Targets

The heater plate is optimized for soldering and reflow work around these solder systems:

- Low-temperature solder: `138 C`
- SnPb eutectic solder: `183 C`
- SAC-class lead-free solder: `217 C`

Operating limits:

- Practical normal maximum: about `250 C`
- Firmware temperature cap: `300 C`
- Forced protection threshold: `360 C`

The resistance target is chosen for useful power through the normal reflow range, not for sustained high power above `250 C`.

## 3) Electrical Target

Nominal cold resistance:

```text
R20 = 3.2 ohm
```

Accepted calibrated board range:

```text
3.1 ohm <= R20 <= 3.3 ohm
```

Each heater plate must be measured during bring-up and the measured cold resistance must be used by firmware power limiting.

The firmware voltage request limit is:

```text
V_cmd <= min(V_source_max, I_source_max * R_estimated(T))
```

Copper temperature coefficient used for first-order estimation:

```text
R(T) = R20 * (1 + 0.00393 * (T - 20))
```

## 4) Power Compatibility

The heater plate is intended to work across these source classes:

- PD 65 W: typically `20 V / 3.25 A`
- PD 100 W: `20 V / 5 A`
- PD 140 W class: `28 V` class operation, current-limited by the discovered source contract

Estimated maximum heater power for `R20 = 3.2 ohm`, after applying the source current contract:

| Heater temperature | Estimated resistance | PD 65 W | PD 100 W | PD 140 W class |
| ---: | ---: | ---: | ---: | ---: |
| `0 C` | `2.95 ohm` | `31 W` | `74 W` | `74 W` |
| `20 C` | `3.20 ohm` | `34 W` | `80 W` | `80 W` |
| `60 C` | `3.70 ohm` | `39 W` | `93 W` | `93 W` |
| `138 C` | `4.68 ohm` | `49 W` | `85 W` | `117 W` |
| `183 C` | `5.25 ohm` | `55 W` | `76 W` | `131 W` |
| `217 C` | `5.68 ohm` | `60 W` | `70 W` | `138 W` |
| `235 C` | `5.90 ohm` | `62 W` | `68 W` | `133 W` |
| `245 C` | `6.03 ohm` | `64 W` | `66 W` | `130 W` |
| `250 C` | `6.09 ohm` | `64 W` | `66 W` | `129 W` |

Low-temperature operation must be voltage-limited because full source voltage can exceed the source current contract before the copper trace heats up. At `20 C`, the expected current-limit voltage is about `10.4 V` for a `3.25 A` source and about `16.0 V` for a `5 A` source.

PD 65 W cold-start compatibility is conditional: at `0 C` and `20 C`, a `12 V` full-on drive exceeds a `3.25 A` source contract. A `3.25 A` source must therefore use a negotiated voltage below the current-limit voltage, or a validated firmware current-limit mode, before static full-on operation. Static `12 V` operation on a `3.25 A` source becomes approximately current-contract safe only once the plate is near `60 C` or hotter.

The fixed-PD table below is intended for firmware voltage-step selection. Temperature rows are rounded and biased toward the points where each fixed voltage crosses about `3 A` or `5 A`, before applying source current limits.

Approximate fixed-voltage threshold guide:

| Fixed voltage | About 5 A | About 3 A | Use note |
| ---: | ---: | ---: | --- |
| `5 V` | below normal range | below normal range | safe low-power fallback |
| `9 V` | below normal range | about `5 C` | below about `5 C`, 9 V is slightly above 3 A |
| `12 V` | below normal range | about `85 C` | below about `85 C`, 12 V is above 3 A |
| `15 V` | about `5 C` | about `165 C` | useful middle step after cold-start limiting |
| `20 V` | about `85 C` | about `296 C` | still about `3.3 A` at `250 C` |
| `28 V` | about `211 C` | about `508 C` | high-power step for EPR-class sources only |

Fixed-voltage current and power estimates for `R20 = 3.2 ohm`, before applying source current limits:

| Heater temperature | Estimated resistance | 5 V | 9 V | 12 V | 15 V | 20 V | 28 V |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `0 C` | `2.95 ohm` | `1.70 A / 8 W` | `3.05 A / 27 W` | `4.07 A / 49 W` | `5.09 A / 76 W` | `6.78 A / 136 W` | `9.50 A / 266 W` |
| `5 C` | `3.01 ohm` | `1.66 A / 8 W` | `2.99 A / 27 W` | `3.98 A / 48 W` | `4.98 A / 75 W` | `6.64 A / 133 W` | `9.30 A / 260 W` |
| `20 C` | `3.20 ohm` | `1.56 A / 8 W` | `2.81 A / 25 W` | `3.75 A / 45 W` | `4.69 A / 70 W` | `6.25 A / 125 W` | `8.75 A / 245 W` |
| `60 C` | `3.70 ohm` | `1.35 A / 7 W` | `2.43 A / 22 W` | `3.24 A / 39 W` | `4.05 A / 61 W` | `5.40 A / 108 W` | `7.56 A / 212 W` |
| `80 C` | `3.95 ohm` | `1.26 A / 6 W` | `2.28 A / 20 W` | `3.03 A / 36 W` | `3.79 A / 57 W` | `5.06 A / 101 W` | `7.08 A / 198 W` |
| `85 C` | `4.02 ohm` | `1.24 A / 6 W` | `2.24 A / 20 W` | `2.99 A / 36 W` | `3.73 A / 56 W` | `4.98 A / 100 W` | `6.97 A / 195 W` |
| `90 C` | `4.08 ohm` | `1.23 A / 6 W` | `2.21 A / 20 W` | `2.94 A / 35 W` | `3.68 A / 55 W` | `4.90 A / 98 W` | `6.86 A / 192 W` |
| `140 C` | `4.71 ohm` | `1.06 A / 5 W` | `1.91 A / 17 W` | `2.55 A / 31 W` | `3.19 A / 48 W` | `4.25 A / 85 W` | `5.95 A / 166 W` |
| `160 C` | `4.96 ohm` | `1.01 A / 5 W` | `1.81 A / 16 W` | `2.42 A / 29 W` | `3.02 A / 45 W` | `4.03 A / 81 W` | `5.64 A / 158 W` |
| `165 C` | `5.02 ohm` | `1.00 A / 5 W` | `1.79 A / 16 W` | `2.39 A / 29 W` | `2.99 A / 45 W` | `3.98 A / 80 W` | `5.57 A / 156 W` |
| `180 C` | `5.21 ohm` | `0.96 A / 5 W` | `1.73 A / 16 W` | `2.30 A / 28 W` | `2.88 A / 43 W` | `3.84 A / 77 W` | `5.37 A / 150 W` |
| `210 C` | `5.59 ohm` | `0.89 A / 4 W` | `1.61 A / 14 W` | `2.15 A / 26 W` | `2.68 A / 40 W` | `3.58 A / 72 W` | `5.01 A / 140 W` |
| `215 C` | `5.65 ohm` | `0.88 A / 4 W` | `1.59 A / 14 W` | `2.12 A / 25 W` | `2.65 A / 40 W` | `3.54 A / 71 W` | `4.95 A / 139 W` |
| `220 C` | `5.72 ohm` | `0.87 A / 4 W` | `1.57 A / 14 W` | `2.10 A / 25 W` | `2.62 A / 39 W` | `3.50 A / 70 W` | `4.90 A / 137 W` |
| `245 C` | `6.03 ohm` | `0.83 A / 4 W` | `1.49 A / 13 W` | `1.99 A / 24 W` | `2.49 A / 37 W` | `3.32 A / 66 W` | `4.64 A / 130 W` |
| `250 C` | `6.09 ohm` | `0.82 A / 4 W` | `1.48 A / 13 W` | `1.97 A / 24 W` | `2.46 A / 37 W` | `3.28 A / 66 W` | `4.60 A / 129 W` |

## 5) Gerber Package

The checked manufacturing package is stored at:

```text
docs/hardware/gerbers/heater-plate-3p2ohm/flux-purr-heater-plate-3p2ohm-gerbers.zip
```

Package SHA-256:

```text
b9242944dca5ce4694d6f99190d23b1e1a734579c363672fb13361258e878057
```

Key parsed dimensions from the package:

| Item | Value |
| --- | ---: |
| Board outline | about `56 mm x 56 mm` |
| Mounting holes | four `3.0 mm` NPTH holes |
| Heater trace width | `0.40 mm` |
| Heater routed length | `2570.05 mm` |
| Estimated cold resistance | `3.16 ohm ~ 3.23 ohm` |
| Heater trace pitch | about `1.0 mm` |
| Copper-to-copper spacing | about `0.6 mm` |
| Heater copper to board edge | about `1.3 mm` minimum |

The Gerber uses the lower two circular pads as the heater supply contacts. The upper circular pads and side pads are isolated copper features in the checked package and must not be used as the heater supply path.

## 6) Surface Finish and Solder Mask

Recommended solder mask:

- Black solder mask on the circuit side
- Heater traces covered by solder mask
- Circular pads opened in the checked package
- Only the lower circular pads are heater supply pressure contacts

Preferred contact-pad finish:

- `ENIG` for the lowest-maintenance pressure-contact surface

Low-cost DIY contact-pad options:

- `OSP`
- Bare copper

For low-cost builds, treat OSP as a temporary shipping and handling protection for the copper surface. The contact pads remain maintenance items after repeated heat cycles.

`HASL` is not selected for the pressure-contact pads because tin can soften, creep, oxidize, or reflow in the intended operating temperature range.

## 7) Contact Mechanics

The copper posts must provide a stable low-resistance pressure contact:

- Use flat, clean copper-post contact faces.
- Maintain preload with a spring, belleville washer, spring contact, or another elastic structure.
- Avoid relying only on a one-time rigid screw preload.
- Measure contact voltage drop at operating current after assembly.
- Repeat the voltage-drop measurement after thermal cycling.

Contact voltage drop is part of the heater power path and must be treated as a bring-up measurement, not as a fixed design constant.

## 8) Manufacturing Notes

Order notes should include:

```text
Single-sided aluminum-core PCB heater, 1 oz copper, black solder mask on the circuit side.
Heater traces are covered by solder mask. Circular pads are exposed in the checked Gerber package.
Only the lower circular pads are heater supply pressure-contact pads; upper exposed circular pads are isolated no-connect features.
Target cold heater resistance is 3.2 ohm after fabrication.
Intended heater operation is up to about 250 C, with firmware cap at 300 C and forced protection at 360 C.
Confirm solder mask and dielectric suitability for repeated thermal cycling in this use case.
```
