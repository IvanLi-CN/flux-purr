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

- flagship tuning targets: `60 / 140 / 220°C`
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

When the bundle is a review-only checkpoint rather than a committed accepted baseline, keep the same four-file layout and mark it explicitly:

- top-level `kind=thermal_self_test_preliminary_bundle`
- top-level `bundleDisposition=preliminary_review`
- top-level `acceptedProfileRole=review_candidate_snapshot`
- `thermal-profile.accepted.json` means the current review candidate snapshot only; it is not a committed accepted baseline and it is not evidence that EEPROM has been saved

The owner-facing compliant preliminary review bundle is now regenerated through the Rust CLI:

- `flux-purr thermal report rerender-legacy --legacy-bundle-dir <dir> [--output-dir <dir>]`

When an older `preliminary-review-*` legacy directory already exists, rerender it through this Rust CLI path rather than treating the legacy `run.bundle.json` as the owner-facing final report. The same command also accepts an already-compliant `thermal_self_test_preliminary_bundle` input and rewrites it into a fresh output directory, so the final `index.html + run.bundle.json + samples.ndjson + thermal-profile.accepted.json` package can stay on the Rust-owned path.

Merged preliminary review bundles may also attach a per-target `holdCheck` block to the same `thermal_self_test_preliminary_bundle` payload. That block should summarize the single-target `60s` hold confirm for the same target and carry:

- `confirmRunId`
- `passed`
- `failureReason`
- `holdSeconds`
- `maxOvershootC`
- `holdPeakToPeakC`
- `firstHoldAtMs`
- `holdMedianOutputPermille`
- `holdP90OutputPermille`
- `approachSource`
- `holdSource`
- `sourceRunPath`
- `stopReason`

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
- if RTD enters fault, owner-facing display keeps the last valid readout instead of synthesizing `0°C`

Current control-path truth is:

- control loop frequency: `20Hz`
- RTD oversampling per control cycle: `64` kept conversions
- settle discard per cycle: `8`
- minimum valid samples: `48`
- default `tempFilterAlphaPermille`: `750`

Do not insert any additional multi-sample window, median, clamp, or rate limiter before the EMA path. Those stages distort heating and cooling rate readback and make the controller react to an artificial temperature trace.

Offline retuning replays the existing `run.json` and `samples.ndjson` pair, writes `run.replayed.json` and `thermal-profile.replayed.candidate.json`, and may optionally apply the replayed candidate back as a RAM-only preview. When `--apply-preview` is used, the CLI must write replay artifacts first, send `thermalControlProfile.op=preview`, confirm `thermalControlProfilePreview=true` from status readback, and record the attempt in `run.replayed.json.applyPreview`. Replay apply must not save EEPROM.

At PPS transition boundaries:

- owner-facing temperature must continue to update from each valid RTD sample
- only controller EMA and slope may remain guarded across the transition window

### Thermal-runaway attention and measurement-fault recovery

Current runtime truth separates thermal-runaway attention from measurement protection:

- `temp >= 420°C` replays the thermal-runaway cue every `1s`
- after temperature falls below `420°C`, an unacknowledged runaway exposes `faultAttentionPending=true` and uses a `10s` reminder cadence
- front panel input and runtime/CLI/app `faultAttentionAcknowledged=true` are equivalent acknowledgement paths, but acknowledgement never bypasses active absolute overtemperature cutoff
- `sensor-open`, `sensor-short`, and `adc-read-failed` stop heating without buzzer attention or pending reminder

Thermal tuning automation should treat measurement faults and runaway attention as distinct recovery paths:

1. Stop heating for the interrupted sub-step and keep active cooling enabled.
2. Poll runtime status on the same owner-authorized device until `heaterFaultReason` clears and runtime exits `fault`.
3. Only when runtime reports a real `faultAttentionPending=true` thermal-runaway reminder, send `faultAttentionAcknowledged=true` before the next test.
4. Clear the RAM thermal preview that belonged to the interrupted attempt.
5. Retry only the same failed sub-step once.

If three consecutive valid tests still carry transient sensor-fault or reminder evidence, stop the sprint and require manual inspection. Record the affected attempts and rerun those same attempts only after human confirmation that the hardware path is healthy again.

`runtime-rearm-attempts` is a bounded autonomy knob, not a license for open-ended retries. The host may automatically recover only the same target after recoverable runtime interruptions, must return the runtime to cooldown before retrying, and must leave a failed receipt or run evidence behind when recovery does not converge.

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

The candidate generator is deliberately not a general parameter search. Every valid scout, batch candidate, and confirm is retained in the canonical review bundle; rejected candidates remain visible with their effective point, result classification, samples, and source telemetry so the tuning direction can be audited. An interrupted or environment-invalid attempt may be shown as excluded audit metadata but must not affect scoring or the valid test count.

A stage produces three classes of evidence, and each class should update only the fields that can explain it:

- first classify the `warmupExitedAtMs -> firstHoldAtMs` approach trace against an ideal reference curve built from `approachStartTempC -> targetTempC`; the canonical fit object is target error / normalized progress, not a standalone `target - ambient` time series
- insufficient near-target heat: raise `holdPowerPermille`, then `approachFloorPowerPermille` and `holdReheatPowerPermille` from measured sustain gap
- excess stored heat: increase `brakeDistanceCentiC`, increase damping, and increase predictive lead
- hold ripple: rebase hold power on measured equilibrium, narrow the reheat gap, and adjust hold-entry / reentry dynamics

Ambient temperature is still useful, but only as an optional compensation term layered onto the same approach curve. If the sample stream does not carry a stable ambient field, tuning must continue from the approach-start baseline and record that fit basis explicitly in the run analysis.

Use sparse focused tuning during iteration. Reserve the full supported ladder for final acceptance.

For the flagship target set `60 / 140 / 220°C`, use a fixed budgeted workflow per target:

1. tuning scout
2. target-local retune and one evidence-specific predicted point
3. one batch comparison of `current` and the predicted point
4. repeat the same target-local scout/retune/batch loop while the per-target budget remains
5. run a `60s` hold confirm every time a promotable candidate clears the gate

The `60 / 220°C` focused re-test uses the same workflow with only those two values passed as anchors, validation targets, and tuning targets. Its seed contains only the requested explicit points; it must not materialize an unrelated interpolation target. A short-scout p2p result cannot create a Hold candidate. Only a candidate with valid `100%` warmup output, the target-specific stable-window gate, and its confirmation margin may be promoted to a `60s` Hold confirm. If a confirm fails thermally while budget remains, keep that failed confirm as valid evidence, use it to seed the next predicted correction through the next scout/batch loop, and continue the same target until it either completes, exhausts the budget, or becomes environment-blocked.

The flagship execution whitelist is fixed:

- run host-side tuning/report/dynamic-gate tests before real HIL
- bind repo-local `flux-purr-devd` to the exact owner-authorized serial path only
- confirm Flux Purr runtime readback shows `selectedMode=100w`, `resolvedBank=pps5a`, and `detectedSourceClass=pps5a` before heating
- confirm IsolaPurr readback still shows `100W`, PD enabled, PPS enabled, `pd_pps_5a=true`, `pps3_limit_ma >= 5000`, and `tps_mode=auto_follow`
- run only the explicit target order, with at most two evidence-specific tuning rounds plus one `60s` confirm per target; a target-local confirm failure ends that target without a recovery scout or another confirm
- keep `warmupPowerPermille=1000` and require actual warmup output to stay at `100%`

The flagship sprint must not:

- add `80 / 100 / 120 / 160 / 180 / 240 / 250°C` runs
- run the full ladder
- collect default `0% / 25% / 50%` approach-only curves
- flash firmware, reset the MCU, change selector, or switch to another serial path
- save `pps5a` EEPROM or freeze a committed accepted baseline
- restart from `60°C` after a later target fails; only the failed sub-step may be retried

The target-specific start condition is fixed:

- when `target < 80°C`, start only after `currentTempC <= 35°C`
- when `target >= 80°C`, start only after `currentTempC <= target - 40°C`

Do not add extra soak time after the threshold is met.

The flagship workflow uses a target-dependent full-speed-to-stable gate:

- `target <= 150°C`: first stable window must start within `10s` after leaving `warmup`
- `target > 150°C`: first stable window must start within `5s` after leaving `warmup`
- the stable window is `10s` continuous sampling with `abs(currentTempC - targetTempC) <= 1.5°C`
- every stage report must include `fullSpeedToStable.limitMs`, `settleTimeMs`, and `failureReason`

Do not run default `0% / 25% / 50%` approach-only characterization inside the flagship sprint. Those curves are diagnostic-only artifacts and should not consume the per-target budget unless explicitly requested.

For candidate tuning:

- warmup itself remains fixed at `100%`
- full-speed timeout without valid hold evidence is treated as approach power / early coast / near-target sustain evidence
- if valid hold samples exist and hold p2p is above limit, do not let full-speed timeout hide hold ripple
- high-temperature low-side deep hold drop must tune hold residency / sustain / lead instead of being classified as residual overshoot
- source/runtime/sample-rate/measurement faults must not mutate the candidate
- distinguish failure before generating a candidate: `missed_lower_band_before_limit`, `missed_upper_band_before_limit`, `stable_window_broke_low`, `stable_window_broke_high`, and `within_gate_low_margin` require different corrections
- a low-side miss with approach already at full power moves only the warmup handoff; a high-side stable-window break changes only braking and approach-tail heat
- require at least `1s` full-speed margin at or below `150°C` and `0.5s` above `150°C` before promoting a short scout result to confirm
- if confirm fails thermally while budget remains, generate the next evidence-specific correction, verify it with the next short scout, and continue iterating until the target completes, the wall-clock budget is exhausted, or the environment blocks further progress

## Validation gates

The saved-profile acceptance contract remains:

- the flagship set `60 / 140 / 220°C` must pass in order before extending to interpolated non-flagship temperatures
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
3. Restart the same IsolaPurr runtime output gate on the same authorized source:
   - `isolapurr power runtime output --url <source-url> --enabled false --json`
   - `isolapurr power show --url <source-url> --json` and confirm `runtime.output_enabled=false`
   - confirm the USB-C output is no longer sourcing power: either `usb_c_actual.status != ok`, or both `usb_c_actual.current_ma=0` and `usb_c_actual.power_mw=0`
   - wait `2s`
   - `isolapurr power runtime output --url <source-url> --enabled true --json`
4. Poll `isolapurr power show --url <source-url> --json` until all of the following are true again:
   - `runtime.output_enabled=true`
   - `tps_mode=auto_follow`
   - `power_watts=100`
   - PD and PPS both enabled
   - `pd_pps_5a=true`
   - `pps3_limit_ma >= 5000`
   - USB-C sample uptime keeps advancing
5. Confirm the exact owner-authorized Flux Purr serial path still exists. If it disappeared or re-enumerated, stop; do not switch ports.
6. Retry only the same failed sub-step, and only once. A second environment failure for the same target becomes `environment_blocked`.
7. Count the recovery time inside the same per-target `20min` budget.

Use this procedure only when source telemetry proves the source is stuck or stale. Do not use it to mask a controller, sensor, or runtime defect.

### Recovering a transient measurement fault

If a run stops on `sensor-open`, `sensor-short`, or `adc-read-failed`, do not immediately classify the target as a thermal tuning failure:

1. Stop heating for the current sub-step and keep active cooling enabled.
2. Poll runtime status on the same owner-authorized device until `heaterFaultReason` clears and runtime exits `fault`.
3. Do not expect or acknowledge `faultAttentionPending`; measurement faults do not enter the buzzer attention state machine.
4. Clear the RAM thermal preview that belonged to the interrupted attempt.
5. Retry only the same failed sub-step once.

Use this path only for transient measurement warnings that clear on the same hardware path. Do not use it to hide repeated sensor faults, runtime resets, or source-side capability loss.

Do not add a `sensor-glitch` fault based on adjacent temperature or raw ADC deltas. A PPS request or VIN transition may trigger an immediate RTD reread, but a valid reread must continue through the established display/control sampling path; only open, short, ADC read failure, and absolute overtemperature are hard protection inputs.

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
- `thermal-self-test-runs/preliminary-pd100w-pps5a-60-140-220-20260717/`
- `thermal-self-test-runs/variants_100c_v6_hold220_cutoff90.json`
