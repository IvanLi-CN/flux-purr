# Heater Plate `heater-5p6-3p2`

This file defines the `56 mm x 56 mm`, `3.2 ohm` heater-plate version. Shared heater-plate rules live in [../heater-plate-design.md](../heater-plate-design.md).

## 1) Profile

Nominal cold resistance:

```text
R20 = 3.2 ohm
```

Accepted calibrated board range:

```text
3.1 ohm <= R20 <= 3.3 ohm
```

## 2) Power Compatibility

The `heater-5p6-3p2` version is intended to work across these source classes:

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
| `295 C` | `6.66 ohm` | `0.75 A / 4 W` | `1.35 A / 12 W` | `1.80 A / 22 W` | `2.25 A / 34 W` | `3.00 A / 60 W` | `4.21 A / 118 W` |
| `300 C` | `6.72 ohm` | `0.74 A / 4 W` | `1.34 A / 12 W` | `1.79 A / 21 W` | `2.23 A / 33 W` | `2.98 A / 60 W` | `4.17 A / 117 W` |
| `350 C` | `7.35 ohm` | `0.68 A / 3 W` | `1.22 A / 11 W` | `1.63 A / 20 W` | `2.04 A / 31 W` | `2.72 A / 54 W` | `3.81 A / 107 W` |

## 3) Gerber Package

The checked manufacturing package is stored at:

```text
docs/hardware/gerbers/heater-plate-5p6cm-3p2ohm/flux-purr-heater-plate-5p6cm-3p2ohm-gerbers.zip
```

Package SHA-256:

```text
74600dd40e183d3de0b55f0e0bdb7623979e4d34b551e5f9761e83998d37ea30
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
