---
status: active
related_specs:
  - docs/specs/q2aw6-heater-pid-frontpanel-runtime/SPEC.md
  - docs/specs/m8r4q-real-control-plane-runtime/SPEC.md
---

# Thermal control profile preview and self-test

Thermal control tuning should be treated as a measured control workflow, not a fixed duty tweak.

The firmware exposes a conservative default controller, a RAM-only profile preview, and an explicit save path for the active thermal control profile. A profile point describes:

- `targetTempC`
- `brakeDistanceCentiC`
- `approachPowerPermille`
- `holdPowerPermille`

The runtime interpolates between points. Missing points fall back to conservative defaults. Preview state is intentionally volatile. `thermalControlProfile.op=save` writes the active profile through the normal EEPROM-backed runtime config path; `op=clear_saved` removes the EEPROM-backed profile.

## Power abstraction

The controller should output equivalent heat power, not backend-specific voltage or PWM details.

- PPS backend: map requested power to a `100 mV` aligned CH224Q PPS voltage request and keep the MOS gate static.
- Fixed PD backend: choose the nearest PDO that is not below the equivalent voltage target, then use MOS PWM to synthesize the requested power.
- Current-limit fallback remains a safety boundary. If the available current cannot support the requested PPS voltage, fall back to fixed PD PWM and cap duty by the same current contract.

CH224Q PPS requests use register `0x53` in `100 mV` units. Do not design a PPS hold loop that depends on `20 mV` or AVS `25 mV` steps unless the hardware adapter explicitly supports that path.

## Self-test packet

A useful thermal self-test packet has three files:

- `run.json`: parameters, source identity, target ladder, per-target metrics, validation result, and file paths
- `samples.ndjson`: raw time-series samples with phase, target, source request, status snapshot, and timestamps
- `thermal-profile.candidate.json`: preview/save-compatible profile proposed by the tooling

The default ladder for Flux Purr covers the supported tuning range only:

`50, 100, 120, 150, 180, 200, 210, 220, 250°C`

Do not include `300°C` in first-version thermal self-test acceptance, even if it remains a runtime preset.

## Validation gates

The preview run is acceptable only when every target satisfies:

- maximum overshoot `<= 3.0°C`
- continuous hold peak-to-peak `<= 3.0°C`; the default HIL hold window is `60s`, and if temperature leaves the target stability band, the hold window restarts instead of counting stale samples
- rise time no slower than baseline by more than `15%`

Each stage has a default `300s` safety deadline. If the deadline expires or runtime state is lost too many times, the self-test actively sends `heaterEnabled=false` and stops the ladder instead of moving to the next target.

Failures should report the target temperature and raw samples. The tooling should not save a profile automatically; saving requires an explicit API/CLI action after reviewing the report.

## Hardware boundary

Flux Purr self-test uses the repo-local `flux-purr` CLI through `flux-purr-devd` for the device under test. IsolaPurr is an external bench source and must be controlled through released `isolapurr` / `isolapurr-devd` tools, not source commands or raw local HTTP.

For banana-jack bench output, keep the IsolaPurr USB-C VBUS path disconnected unless the operator explicitly chooses shared USB-C output.
