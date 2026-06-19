---
title: Flux Purr calibration data packet collection
module: device-control
problem_type: workflow
component: host-tools
tags:
  - calibration
  - status-capture
  - usb-lease
  - runtime-control
  - heater-curve
status: active
related_specs:
  - docs/specs/jt8r2-adc-calibration-control-plane/SPEC.md
  - docs/specs/m8r4q-real-control-plane-runtime/SPEC.md
---

# Flux Purr calibration data packet collection

This workflow captures one heater run as a reusable calibration data packet and keeps the raw time series, the derived summary, and the operator boundary together.

## When to use

Use this workflow when you need a calibration packet that can be replayed, curve-fit, or compared across current setpoints.

The packet is meant to answer two questions:

1. Did the heater run reach the cutoff temperature cleanly?
2. What did the heater's effective resistance do as temperature rose?

## Operator setup

- Use the authorized Flux Purr USB port for the target board.
- Put the external bench source into manual constant-current mode.
- Force the USB-C path on and hold the source at `20V`.
- Run each current setpoint as a separate capture.
- Keep `3A` and `3.25A` in separate runs so each packet stays independently analyzable.

Only describe the device classes in this document:

- Flux Purr target board
- IsolaPurr bench source

Do not record serial IDs, device IDs, or port enumerations in the reusable workflow text.

## Capture contract

- Poll interval: `500ms`
- Heater target during capture: `270C`
- Automatic heat cutoff: `250C`
- Per-run timeout: `300s`
- Start temperature gate: `<= 40C`
- Heater control mode: fully on
- External source mode: manual CC

The host tool should emit:

- `run.json` with summary, stop reason, runtime verification, and series statistics
- `samples.ndjson` with one raw time-series sample per poll

Each sample should include:

- timestamp
- elapsed time
- heater state
- Flux Purr temperature and ADC readbacks
- IsolaPurr voltage/current/power readbacks
- run phase and stop diagnostics

## Derived analysis

For this workflow, the useful derived curve is the heater's effective resistance:

`R_effective = V_readback / I_readback`

Treat that as a run-time diagnostic curve, not as a material constant.

### Observed shape

The runs show a noisy startup region and a much smoother high-temperature region:

- Below about `80C`, the curve is dominated by transient startup behavior.
- From about `120C` upward, the effective resistance becomes much steadier.
- Between `120C` and `250C`, the curve rises gently from about `6.0 ohm` to about `7.1 ohm`.

### Practical binning

| Temp band | 3A effective R | 3.25A effective R | Notes |
| --- | ---: | ---: | --- |
| `0-40C` | `29.24 ohm` | `38.22 ohm` | startup transient, not fit-friendly |
| `40-80C` | `27.52 ohm` | `12.14 ohm` | still transient-heavy |
| `80-120C` | `11.76 ohm` | `12.77 ohm` | transition region |
| `120-160C` | `6.08 ohm` | `5.99 ohm` | stable enough for coarse fitting |
| `160-200C` | `6.59 ohm` | `6.48 ohm` | stable mid-band |
| `200-230C` | `6.94 ohm` | `6.84 ohm` | late rise begins |
| `230-250.5C` | `7.11 ohm` | `7.08 ohm` | near cutoff, good comparison band |

## Real run results

### 3A run

- Reached `250.6C`
- Stopped by temperature cutoff
- Sample count: `417`
- Mean interval: `521.5ms`
- Final IsolaPurr readback: about `20.0V`, `2.82A`

### 3.25A run

- Reached `250.6C`
- Stopped by temperature cutoff
- Sample count: `476`
- Mean interval: `512.7ms`
- Final IsolaPurr readback: about `20.0V`, `2.79A`

## What makes the packet usable

Keep both outputs together:

- raw time series for replay and later fitting
- summary for stop condition, cadence, and boundary verification

This is the minimum useful packet shape for later analysis. A plain success/fail note is not enough.

## Saving the heater curve

The captured packet becomes operational only after the stable resistance points are converted into a heater curve package:

```json
{
  "package": {
    "points": [
      { "tempCentiC": 13977, "resistanceMilliohms": 6033 },
      { "tempCentiC": 18153, "resistanceMilliohms": 6522 },
      { "tempCentiC": 21601, "resistanceMilliohms": 6882 },
      { "tempCentiC": 24123, "resistanceMilliohms": 7094 },
      null,
      null,
      null,
      null
    ]
  }
}
```

Use `preview` before `save`:

- `preview` loads the curve into RAM and immediately affects heater power limiting for live testing.
- `save` copies the preview curve into active EEPROM-backed configuration.
- Reboot restores only the saved active curve; unsaved preview state is intentionally lost.

The stable package should omit the startup transient bins. The practical curve above starts around `140C`, where both current-setpoint runs agree well enough for power-limit sanity checks.

## Guardrails

- Do not rely on duty-cycle control for this workflow.
- Do not split the capture into mixed-current runs.
- Do not treat startup bins as fit anchors.
- Do not expose device IDs in the reusable solution.
- Do not call `save` until preview behavior has been verified on the target board.

## References

- [Calibration control plane spec](../../specs/jt8r2-adc-calibration-control-plane/SPEC.md)
- [Real control plane runtime spec](../../specs/m8r4q-real-control-plane-runtime/SPEC.md)
