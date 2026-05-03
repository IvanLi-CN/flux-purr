# Heater Plate `heater-5p6-4p5-original`

This file defines the original `56 mm x 56 mm`, `4.5 ohm` heater-plate version. Shared heater-plate rules live in [../heater-plate-design.md](../heater-plate-design.md).

## 1) Profile

Nominal cold resistance:

```text
R20 = 4.5 ohm
```

Expected calibrated board range:

```text
4.3 ohm <= R20 <= 4.8 ohm
```

Boards outside this range require a separate firmware profile or a hardware review.

## 2) Power Compatibility

The original `4.5 ohm` version draws less current at the same fixed voltage and is easier to keep inside a `3 A` source at low fixed-PD voltages. It also heats more slowly at the same source voltage.

Approximate fixed-voltage threshold guide:

| Fixed voltage | About 5 A | About 3 A | Use note |
| ---: | ---: | ---: | --- |
| `5 V` | below normal range | below normal range | safe low-power fallback |
| `9 V` | below normal range | below normal range | below 3 A through the normal range |
| `12 V` | below normal range | below normal range | below 3 A through the normal range |
| `15 V` | below normal range | about `50 C` | cold operation is slightly above 3 A |
| `20 V` | below normal range | about `143 C` | useful high step after warm-up |
| `28 V` | about `82 C` | about `293 C` | EPR-class step, current-limited before warm-up |

Fixed-voltage current and power estimates for `R20 = 4.5 ohm`, before applying source current limits:

| Heater temperature | Estimated resistance | 5 V | 9 V | 12 V | 15 V | 20 V | 28 V |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `0 C` | `4.15 ohm` | `1.21 A / 6 W` | `2.17 A / 20 W` | `2.89 A / 35 W` | `3.62 A / 54 W` | `4.82 A / 96 W` | `6.75 A / 189 W` |
| `20 C` | `4.50 ohm` | `1.11 A / 6 W` | `2.00 A / 18 W` | `2.67 A / 32 W` | `3.33 A / 50 W` | `4.44 A / 89 W` | `6.22 A / 174 W` |
| `50 C` | `5.03 ohm` | `0.99 A / 5 W` | `1.79 A / 16 W` | `2.39 A / 29 W` | `2.98 A / 45 W` | `3.98 A / 80 W` | `5.57 A / 156 W` |
| `60 C` | `5.21 ohm` | `0.96 A / 5 W` | `1.73 A / 16 W` | `2.30 A / 28 W` | `2.88 A / 43 W` | `3.84 A / 77 W` | `5.38 A / 151 W` |
| `80 C` | `5.56 ohm` | `0.90 A / 4 W` | `1.62 A / 15 W` | `2.16 A / 26 W` | `2.70 A / 40 W` | `3.60 A / 72 W` | `5.03 A / 141 W` |
| `85 C` | `5.65 ohm` | `0.89 A / 4 W` | `1.59 A / 14 W` | `2.12 A / 25 W` | `2.66 A / 40 W` | `3.54 A / 71 W` | `4.96 A / 139 W` |
| `138 C` | `6.59 ohm` | `0.76 A / 4 W` | `1.37 A / 12 W` | `1.82 A / 22 W` | `2.28 A / 34 W` | `3.04 A / 61 W` | `4.25 A / 119 W` |
| `180 C` | `7.33 ohm` | `0.68 A / 3 W` | `1.23 A / 11 W` | `1.64 A / 20 W` | `2.05 A / 31 W` | `2.73 A / 55 W` | `3.82 A / 107 W` |
| `190 C` | `7.51 ohm` | `0.67 A / 3 W` | `1.20 A / 11 W` | `1.60 A / 19 W` | `2.00 A / 30 W` | `2.66 A / 53 W` | `3.73 A / 104 W` |
| `217 C` | `7.98 ohm` | `0.63 A / 3 W` | `1.13 A / 10 W` | `1.50 A / 18 W` | `1.88 A / 28 W` | `2.51 A / 50 W` | `3.51 A / 98 W` |
| `250 C` | `8.57 ohm` | `0.58 A / 3 W` | `1.05 A / 9 W` | `1.40 A / 17 W` | `1.75 A / 26 W` | `2.33 A / 47 W` | `3.27 A / 92 W` |
| `293 C` | `9.33 ohm` | `0.54 A / 3 W` | `0.96 A / 9 W` | `1.29 A / 15 W` | `1.61 A / 24 W` | `2.14 A / 43 W` | `3.00 A / 84 W` |
| `300 C` | `9.45 ohm` | `0.53 A / 3 W` | `0.95 A / 9 W` | `1.27 A / 15 W` | `1.59 A / 24 W` | `2.12 A / 42 W` | `2.96 A / 83 W` |
| `350 C` | `10.34 ohm` | `0.48 A / 2 W` | `0.87 A / 8 W` | `1.16 A / 14 W` | `1.45 A / 22 W` | `1.93 A / 39 W` | `2.71 A / 76 W` |

## 3) Gerber Package

The checked manufacturing package is stored at:

```text
docs/hardware/gerbers/heater-plate-5p6cm-4p5ohm-original/flux-purr-heater-plate-5p6cm-4p5ohm-original-gerbers.zip
```

Package SHA-256:

```text
690eb6daf2a8ea5fc04415b0826d52d519622f332e638756355c3671bcd83ff2
```

Key parsed dimensions from the package:

| Item | Value |
| --- | ---: |
| Board outline | about `56 mm x 56 mm` |
| Mounting holes | four `3.0 mm` NPTH holes |
| Heater trace width | `0.30 mm` |
| Heater routed length | `2742.89 mm` |
| Estimated cold resistance | about `4.50 ohm` |
| Heater trace pitch | about `1.0 mm` |
| Copper-to-copper spacing | about `0.7 mm` |
| Heater copper to board edge | about `1.3 mm` minimum |
