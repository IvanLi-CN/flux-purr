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

Estimated maximum heater power for `R20 = 3.2 ohm`:

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
- Copper-post contact pads opened

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
Heater traces are covered by solder mask. Lower circular contact pads are exposed pressure-contact pads.
Target cold heater resistance is 3.2 ohm after fabrication.
Intended heater operation is up to about 250 C, with firmware cap at 300 C and forced protection at 360 C.
Confirm solder mask and dielectric suitability for repeated thermal cycling in this use case.
```
