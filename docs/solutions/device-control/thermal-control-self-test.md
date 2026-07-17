---
title: Thermal control profile preview and self-test
module: device-control
problem_type: thermal-hil
component: firmware-cli-devd-isolapurr
tags:
  - thermal-control
  - pd-pps
  - isolapurr
  - hil
status: active
related_specs:
  - docs/specs/q2aw6-heater-pid-frontpanel-runtime/SPEC.md
  - docs/specs/m8r4q-real-control-plane-runtime/SPEC.md
---

# Thermal control profile preview and self-test

## Context

Thermal control tuning is a measured control workflow, not a fixed duty tweak.

Flux Purr exposes:

- a conservative firmware default controller
- a RAM-only `thermalControlProfile.op=preview`
- an EEPROM-backed `save` / `clear_saved` path
- two persisted thermal banks: `pps3a` and `pps5a`
- a user-facing profile mode: `auto | 65w | 100w`

`auto` resolves the bank from the source capability class. Explicit `65w` and `100w` are forced selections and do not auto-fallback.

The persisted profile is sparse by design:

- preview may keep up to `10` RAM slots
- EEPROM persists at most `6` populated anchors per bank
- EEPROM v2 uses fixed `2 KiB` slots
- read order is `2 KiB v2 -> 1 KiB v1 -> 512 B legacy`
- old single-profile records migrate into `pps3a` with mode `65w`

The current tuning and acceptance convention is:

- sparse tuning anchors: `60 / 140 / 220°C`
- supported explicit ladder: `60 / 100 / 140 / 180 / 220 / 250°C`
- `300°C` remains outside first-version acceptance

## Artifact model

One live self-test run produces a transient working packet:

- `run.json`
- `samples.ndjson`
- `thermal-profile.candidate.json`

The live working packet must not generate or retain any extra local browser page. Transient run/replay/batch directories stay data-only.

For long-lived comparison and retuning, freeze an accepted baseline bundle instead. The canonical browser-openable bundle layout is:

- `index.html`
- `run.bundle.json`
- `samples.ndjson`
- `thermal-profile.accepted.json`

Do not treat local MHTML snapshots as the primary artifact. The canonical owner-facing deliverable is the HTML bundle.

For approach-curve fitting, use the same canonical bundle shape. A dedicated `thermal_approach_characterization` bundle may freeze per-target approach-only traces without turning the live run directory into another report format.

The report and bundle must preserve source-aware metadata:

- `selectedMode`
- `resolvedBank`
- `detectedSourceClass`
- source preset / readback metadata
- per-stage `analysis.approachSource`
- per-stage `analysis.holdSource`

Each stage summary carries `sampleCount` plus `min/max/avg/first/last` for source voltage, current, and power so a tuning failure can be separated from a source-side failure without replaying raw NDJSON by hand.

## Current truth that future tuning must preserve

### Warmup semantics

Warmup is no longer a profile-tunable effective output level.

Current firmware truth is:

- while the heater state machine remains in `warmup`, output is forced to `100%`
- PPS request follows the current temperature-and-source safety ceiling
- host readback, candidate import, replay, and report generation must all materialize `warmupPowerPermille=1000` as the only effective value

If a host path preserves an older reduced warmup value, it creates a false readback mismatch or an owner-facing report that misstates what firmware actually did.

### RTD path and temperature ownership

The RTD path has two distinct consumers and they must stay separated:

- owner-facing temperature uses the current valid RTD sample
- controller temperature uses the EMA state

Current control-path truth is:

- control loop frequency: `20Hz`
- RTD oversampling per control cycle: `64` kept conversions
- settle discard per cycle: `8`
- minimum valid samples: `48`
- default `tempFilterAlphaPermille`: `750`

Do not insert any additional multi-sample window, median, clamp, or rate limiter before the EMA path. Those stages distort heating and cooling rate readback and make the controller react to an artificial temperature trace.

At PPS transition boundaries:

- owner-facing temperature must continue to update from each valid RTD sample
- only controller EMA and slope may remain guarded across the transition window

### Phase-transition and low-temperature guardrails

The following controller guardrails are now part of the reusable solution:

- `warmup -> approach` must not trigger from a single actual-sample spike while filtered temperature still lags
- `approach -> hold` must not carry the controller into hold while actual temperature is still outside the configured hold-entry band
- predictive coast may pull output to `0%`, but only under the real hold-entry and projection constraints
- low-temperature bounded residual heat must not be misclassified as hold-entry carry

### Source-side safety boundary

Thermal runs must evaluate the source and heater together.

Current reusable rule set:

- capability-class resolution is based on configured / advertised source capability, not live current
- explicit `65w` / `100w` are forced mode selections
- safety limiting still uses live current plus `heaterCurrentReserveMa`
- current reserve remains part of the persisted settings and protects the board from consuming the full advertised source budget

## Seed and baseline selection

The current bank-aware defaults are:

- `pps3a`: seed from the committed 65W accepted bundle
- `pps5a`: seed from the committed 100W accepted bundle when it exists
- `pps5a` fallback before accepted 100W baseline exists: `thermal-self-test-runs/variants_100c_v6_hold220_cutoff90.json`

Until a committed `pps5a` accepted baseline exists, do not treat a stale saved `pps5a` EEPROM profile as current truth. Focused 100W retuning should start from an explicit preview seed, not from an arbitrary previously saved bank.

## Candidate identification

The candidate generator is deliberately not a general parameter search.

A stage produces three classes of evidence, and each class should update only the fields that can explain it:

- first classify the `warmupExitedAtMs -> firstHoldAtMs` approach trace against an ideal reference curve built from `approachStartTempC -> targetTempC`; the canonical fit object is target error / normalized progress, not a standalone `target - ambient` time series
- insufficient near-target heat: raise `holdPowerPermille`, then `approachFloorPowerPermille` and `holdReheatPowerPermille` from measured sustain gap
- excess stored heat: increase `brakeDistanceCentiC`, increase damping, and increase predictive lead
- hold ripple: rebase hold power on measured equilibrium, narrow the reheat gap, and adjust hold-entry / reentry dynamics

Ambient temperature is still useful, but only as an optional compensation term layered onto the same approach curve. If the sample stream does not carry a stable ambient field, tuning must continue from the approach-start baseline and record that fit basis explicitly in the run analysis.

Use sparse focused tuning during iteration. Reserve the full supported ladder for final acceptance.

For dedicated approach characterization:

- collect one `zero_coast` curve and one `half_floor_50` curve for each target temperature
- start each curve at `warmup -> approach` handoff
- stop the curve only after the sample stream shows both:
  - first entry into the target band
  - a visible rollback from the peak while still remaining in-band
- reject any trace that reaches `hold` before that rollback evidence exists
- use the `zero_coast` approach duration as the hard limit gate
- use the `half_floor_50` approach duration as the preferred target gate

If the brake search times out before entering the band, or never even reaches `approach`, classify the result as `more_heat`. Do not let those cases fall back to a neutral direction; otherwise high-temperature characterization can incorrectly jump back toward larger brake distances and waste real HIL time.

## Validation gates

The saved-profile acceptance contract remains:

- maximum overshoot `<= 3.0°C`
- once hold sampling starts, continuous `60s` hold peak-to-peak `<= 3.0°C`
- each stage stops on runtime reset, heater disarm, target mismatch, mode mismatch, source fault, or deadline expiry

Sampling and recording rules remain:

- default host sampling interval: `300ms`
- acceptance floor: `3Hz`
- accepted comparison bundles may sample faster, including `100ms`
- source telemetry that does not advance for `2s` must be rejected

The one-minute hold window starts when firmware reports `heaterControlPhase=hold` and then continues for the full window even if the controller later leaves hold.

## Failure classification

Thermal tuning and source failures must not be conflated.

Treat the run as a source-side failure, not a thermal-profile result, when any of the following holds:

- source capability readback does not match the intended class
- Flux Purr requests more than `5V`, but source telemetry remains near `5V`
- source telemetry stops advancing
- the source must be power-cycled before PPS negotiation recovers

Treat the run as a measurement-chain failure, not a tuning result, when any of the following holds:

- RTD open / short / ADC read fault occurs
- owner-facing temperature shows physically impossible discontinuities
- front-panel / runtime temperature stops following valid RTD samples

Only classify the result as a tuning issue when the source and measurement paths remain healthy.

## Recovery procedures

### Recovering a stuck source output

If the source remains at a stale low-voltage state or otherwise fails to follow a higher PPS request:

1. Stop heating and keep the run unsaved.
2. Record the failure as a source-side issue, not as thermal-profile evidence.
3. Power-cycle the source output or the upstream USB-C power path.
4. Wait for source telemetry and capability readback to become current again.
5. Recheck `selectedMode`, `resolvedBank`, and `detectedSourceClass` before resuming the thermal run.

Use this procedure only when source telemetry proves the source is stuck or stale. Do not use it to mask a controller, sensor, or runtime defect.

### Recovering a dead local `devd`

If the local daemon stops serving the active hardware path:

1. Confirm the owner-authorized serial path is still the same authorized port.
2. Confirm the current daemon bind is unhealthy.
3. Restart the repo-local daemon with explicit `--bind`, `--serial-port`, and `--artifact-root`.
4. Recheck daemon health, then recheck device status through the same daemon instance.
5. If the authorized serial path disappeared and only a re-enumerated path is present, stop. Do not switch ports without explicit authorization.

## Guardrails / reuse notes

- Keep `preview` as a single RAM overlay.
- Always make `save` / `clear_saved` bank-aware.
- Do not silently reinterpret explicit `65w` or `100w` as another bank.
- Do not use live current to decide `pps3a` vs `pps5a`; use the configured source capability class.
- Do not let owner-facing temperature fall back to guarded control temperature.
- Do not treat transient experimental preview seeds as accepted truth.

## References

- `docs/specs/q2aw6-heater-pid-frontpanel-runtime/SPEC.md`
- `docs/specs/m8r4q-real-control-plane-runtime/SPEC.md`
- `thermal-self-test-runs/baselines/56x56mm-3p2ohm-pd63w-pps3a/accepted-full-range-20hz/`
- `thermal-self-test-runs/approach-characterization-pd100w-pps5a-20260717-final/`
- `thermal-self-test-runs/variants_100c_v6_hold220_cutoff90.json`
