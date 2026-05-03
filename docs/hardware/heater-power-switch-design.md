# Flux Purr Heater Power-Switch Design

This document freezes the current heater switching baseline for the `ESP32-S3FH4R2` board revision.

## 1) Scope

- Heater supply bus: `VBUS`, expected adjustable operating range `12 V ~ 28 V` when the connected source exposes PPS that covers `20 V`; lower-voltage operation is allowed only when the source exposes it and firmware uses it for heater current limiting
- Sources that do not expose PPS covering `20 V` remain compatibility/fallback-only and use the original fixed-PD PWM firmware backend
- Load type: resistive hotplate heater
- Heater plate baseline: [heater-plate-design.md](heater-plate-design.md)
- Switch topology: low-side N-channel MOSFET
- MOSFET gate source: `ESP32-S3 GPIO47` (chip pin `37`)

## 2) Frozen topology

The heater path is:

```text
VBUS -> heater element -> HEATER_SW -> low-side NMOS -> power GND
```

Control path:

```text
ESP32-S3 GPIO47 -> gate resistor -> MOSFET gate
gate -> pulldown -> source/GND
```

This topology is intentionally simple:

- low-side `NMOS` keeps conduction loss lower than a comparable `PMOS`
- the heater is a resistive load, so a simple switch stage is appropriate
- the MCU already owns `GPIO47` (chip pin `37`) as the heater-control output

Upstream protection baseline for the heater branch:

```text
USB-C VBUS pin -> VBUS_RAW -> FUSE_VBUS -> VBUS
VBUS -> TVS_VBUS -> GND
VBUS -> heater copper standoffs / heater branch
```

This means the heater and the rest of the board both run from the fused `VBUS` net rather than from the raw connector pin.

## 3) Approved MOSFET baseline

Current approved part:

- `BUK9Y14-40B,115`

Reason for approval:

- available in the current sourcing context
- Nexperia classifies it as a logic-level MOSFET
- its gate charge is materially lighter than `PSMN014-80YLX`, which makes direct MCU drive more realistic

Important limitation:

- the datasheet does not provide a guaranteed `RDS(on)` number at `3.3 V`
- therefore this is a practical approval under sourcing constraints, not a paper guarantee that every operating corner is ideal

## 4) Gate-drive baseline

Start from this network:

- `R_GATE = 10 Ohm ~ 22 Ohm`
- `R_GPD = 47 kOhm ~ 100 kOhm`

Recommended default:

- `R_GATE = 10 Ohm`
- `R_GPD = 100 kOhm`

Connection:

- `GPIO47 -> R_GATE -> gate`
- `gate -> R_GPD -> source/GND`

Do not leave the gate floating at reset or during boot.

## 5) Heater modulation baseline

Preferred runtime mode:

- CH224Q adjustable-PD request controls heater power across `12 V ~ 28 V`
- `GPIO47` only drives the low-side MOSFET statically off or on
- firmware may enable this mode only after CH224Q power data proves that PPS covers `20 V`
- with the `3.2 ohm` heater plate, PD 65 W cold start is not valid at static `12 V`; firmware must request a lower available voltage or use a validated current-limit fallback until the estimated heater resistance keeps full-on current inside the negotiated contract

Fallback mode:

- if PPS does not cover `20 V`, capability data cannot be read, or adjustable-voltage writes fail, firmware falls back to fixed-PD `GPIO47` PWM
- fallback PWM uses the existing `2 kHz` first-bring-up value

Reasoning:

- PPS/AVS modulation avoids turning the PD input bus into a visibly pulsed high-current load during normal operation
- static MOSFET drive keeps the gate waveform simple in the preferred mode
- the fallback keeps the existing hardware usable with fixed-voltage PD sources

If ADC noise becomes visible in fallback mode, firmware should prefer sampling during the PWM off-window.

## 6) Input decoupling and bulk capacitance

Place a local capacitor group near the heater branch and MOSFET current loop:

- `100 nF / 50 V / X7R`
- `1 uF / 50 V / X7R`
- `10 uF / 50 V / X7R`
- `100 uF / 35 V` low-ESR bulk capacitor
- `220 uF / 35 V` low-ESR bulk capacitor

The bulk capacitor may be:

- aluminum polymer
- solid aluminum
- another clearly low-ESR bulk technology

The archived final controller-board netlist already populates this full stack on `VBUS` near the heater branch:

- `100 nF`
- `1 uF`
- `10 uF`
- `100 uF`
- `220 uF`

Do not treat the bulk capacitors as a replacement for the local MLCC stack. Both are required.

Fuse and TVS baseline:

- add a one-time SMD fuse between `VBUS_RAW` and `VBUS`
- place the fuse on the main board before the route reaches the exposed heater copper standoffs
- add a TVS between `VBUS` and `GND`
- the TVS should protect the board-side `VBUS` net, not sit out on the exposed heater hardware

## 7) Current and power constraints

The PD source and the hotplate switch stage must be checked against the heater current when the MOSFET is fully on.

The supported heater-plate profile currently targets `R_HEATER_COLD ~= 3.2 ohm`. The original archived Gerber estimates to about `4.5 ohm`, but old boards must be measured and assigned a calibrated firmware profile before use.

Hard check:

- `I_ON ~= VIN / R_HEATER_COLD`

The design is only valid if the worst-case on-state current remains inside the negotiated PD current budget with margin for the rest of the board.

First-order full-on power estimate in `pps-mos`:

- `P_ON ~= VIN^2 / R_HEATER`

Firmware maps the controller output to a requested `VIN` instead of treating one PWM duty as equivalent power at every PD voltage. The requested voltage must stay inside the source capability discovered through CH224Q.

## 8) Layout baseline

- keep the `VBUS -> heater -> MOSFET -> GND` high-current loop short and wide
- route heater current return directly into the power-ground region
- do not share the heater return path with ADC quiet-ground routing
- keep the gate trace short and away from the hottest switching copper
- place the MLCC stack and bulk capacitor close to the heater switch loop, not on the far side of the board

Component placement priorities:

- `FUSE_VBUS` should sit close to the point where `VBUS_RAW` enters the board, before the route fans out to heater and regulators
- `TVS_VBUS` should sit close to the protected `VBUS` entry region with a short, low-inductance return to ground
- `R_GATE` must sit close to the MOSFET gate, not close to the MCU pin
- `R_GPD` should also sit close to the MOSFET gate/source so the gate is held in a known state even if the upstream trace is noisy
- the `100 nF / 1 uF / 10 uF` MLCC stack should sit close to the heater current loop entry, ideally near the heater feed and MOSFET drain return path
- the bulk capacitor should sit close to the same heater power loop, not back near the PD connector alone
- if an optional RC snubber footprint is reserved, place it close to the MOSFET drain/source switching loop

## 9) Optional tuning and protection footprints

No freewheel diode is required for the heater branch because the load is treated as resistive, not as a motor or relay coil.

Because the MOSFET is a `40 V` part on a bus that can reach `28 V`, layout discipline matters. It is reasonable to reserve an optional RC snubber footprint for bench tuning if drain overshoot is worse than expected.

This footprint is optional and should not be populated by default without oscilloscope data.

## 10) Validation checklist before PCB freeze

- verify fuse rating against worst-case heater cold-start current
- verify the chosen TVS does not nuisance-conduct at the highest intended PD voltage
- verify MOSFET temperature at the highest intended heater power
- verify drain overshoot at `5 / 9 / 12 / 15 / 20 / 28 V`
- verify the gate waveform with the real `3.3 V` MCU drive
- confirm that worst-case on-state heater current stays inside the PD contract
- confirm that VIN and RTD sampling remain stable in both `pps-mos` and fallback PWM modes

## References

- [Nexperia BUK9Y14-40B product page](https://www.nexperia.com/product/BUK9Y14-40B)
- [Nexperia BUK9Y14-40B datasheet](https://assets.nexperia.com/documents/data-sheet/BUK9Y14-40B.pdf)
- [TI decoupling capacitor notes](https://www.ti.com/content/dam/videos/external-videos/en-us/9/3816841626001/6313253251112.mp4/subassets/notes-decoupling_capacitors.pdf)
