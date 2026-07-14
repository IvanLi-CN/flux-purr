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

Thermal control tuning should be treated as a measured control workflow, not a fixed duty tweak.

The firmware exposes a conservative default controller, a RAM-only profile preview, and an explicit save path for the active thermal control profile. A profile point describes:

- `targetTempC`
- `brakeDistanceCentiC`
- `approachPowerPermille`
- `approachFloorPowerPermille`
- `holdPowerPermille`
- `holdEntryCentiC`
- `holdExitCentiC`
- `holdOnCentiC`
- `holdOffCentiC`
- `overshootCutoffCentiC`
- `holdKpPermillePerC`
- `holdKiPermillePerCTick`
- `holdBlendTicks`
- `holdReheatPowerPermille`

The runtime interpolates between points. `holdPowerPermille` acts as the equilibrium hold baseline for that temperature zone. `approachFloorPowerPermille` acts as the minimum approach-band power floor near target, so high-temperature targets can keep substantially more near-target power than low-temperature targets. `holdReheatPowerPermille` is the stronger under-target sustain/recovery floor: the controller can use it while still below target in the approach band, and again when it falls out of hold and needs to climb back toward target. The point-level damping fields control when the controller enters or exits hold, how hard overshoot cuts power, and how aggressively hold PI works in that zone. Missing point-level damping fields fall back to conservative defaults. Preview state is intentionally volatile. `thermalControlProfile.op=save` writes the active profile through the normal EEPROM-backed runtime config path; `op=clear_saved` removes the EEPROM-backed profile.

Because those extra fields widened each point materially, EEPROM persistence must not blindly serialize all 10 slots every time. The encoder compacts sparse slots and persists at most 6 populated anchors, which fits the TLV one-byte length while still allowing a 10-slot RAM preview. The active record uses two 1 KiB slots, with the former 512 B slots retained as a read-only migration fallback, so six anchors still fit alongside full calibration and Wi-Fi data. Save requests above that limit must fail before reporting success.

The saved profile settings matter as much as the points. In real HIL, `approachMaxTicks=16` only yielded a sub-second approach window on the current firmware main loop, which was too short for the hotplate's thermal inertia. The self-test candidate therefore uses a materially longer approach window and a hold baseline curve that starts from measured equilibrium output rather than a single low global baseline.

Do not rely on a single global near-target power ratio across the full operating range. Real HIL data showed that one global approach floor can only solve one side of the problem at a time:

- low targets overshoot and oscillate if the near-target floor is tuned for `220~250°C`
- high targets stall below setpoint if the near-target floor is tuned for `60~140°C`

Treat near-target approach floor and hold baseline as explicit per-zone control data.

That is still not the whole story. A full real HIL run with the saved-profile path fixed and the longer approach window in place showed:

- the old hot-target timeout shape was corrected and `180 / 220 / 250°C` all reached hold
- low and mid targets still failed on overshoot or hold ripple
- high targets still showed excessive hold peak-to-peak once the controller handed off from approach to hold

The practical lesson is that per-zone `approachFloorPowerPermille` and `holdPowerPermille` are necessary, but not sufficient. Near-target damping itself must also scale with temperature zone and measured equilibrium power. Continuing to tune only point magnitudes while leaving one coarse global hold dynamic in place is not a stable path to acceptance.

The self-test hold metric must also align with the controller's real state machine. The one-minute hold window starts only when firmware reports `heaterControlPhase=hold`; from that point it runs continuously for 60 seconds and includes later excursions back into approach. This prevents a controller from hiding an unstable recovery cycle by leaving hold while temperature is falling.

Dense validation targets and stored profile anchors are different concepts. A validation stage at `80C`, `120C`, `160C`, `200C`, or `240C` may sit between stored anchors. Before arming the heater, the host must interpolate every point field with the same linear ratio, `+0.5` rounding, and field bounds used by firmware, then compare that effective point with runtime readback. Falling back to a temperature-band default on the host side creates a false readback mismatch and prevents dense HIL from running at all.

Two real HIL runs after point-level damping was added made the next gap explicit:

- high-temperature target acquisition is now good enough to reach and hold `180 / 220 / 250°C`
- low and mid targets still overshoot or ring because the hotplate's residual heat is not captured by static point values alone
- another round of point retuning changed which stages failed, but did not eliminate the pattern

That is the sign to move beyond static point tuning and introduce an inertia-aware near-target term, for example a slope/residual-heat model or a hold baseline explicitly tied to measured equilibrium power and temperature rise rate.

After the predictive lead term was added, a full real HIL run confirmed that the overshoot problem was largely solved but the hold-ripple problem remained:

- `60°C` passed both overshoot and hold ripple
- `100 / 140 / 180 / 220 / 250°C` all still failed on hold peak-to-peak, even though their maximum overshoot stayed inside the acceptance limit

That changes the tuning target. The remaining work is no longer "make braking stronger". It is "shape reheat and equilibrium hold output so the controller does not fall into a wide hold cycle once the first plateau is reached".

The next controller revision tightened the `Approach -> Hold` gate so predictive braking no longer
pushes the state machine into hold while actual temperature is still outside the configured
`holdEntryCentiC` band, and it stopped carrying approach output into hold unless both actual and
projected error still justify that preload. A second follow-up added an explicit predictive coast:
inside `Approach`, once the lead term projects a target crossing, output drops to `0%` and the
usual under-target floor is not allowed to reassert itself.

That did not clear acceptance yet, but it materially changed the real HIL shape. The focused run
`thermal-1783566799182-serial-303a-1001-d0-cf-13-08-a1-48` on authorized port
`/dev/cu.usbmodem21221401` with IsolaPurr `856a141cdbd4` LAN source `http://192.168.31.122` measured:

- `100°C`: `maxOvershootC=7.9`, `holdPeakToPeakC=7.5`, `residualHeatAfterHoldEntryC=7.5`
- `180°C`: `maxOvershootC=4.6`, `holdPeakToPeakC=3.8`, `residualHeatAfterHoldEntryC=3.8`
- `220°C`: `maxOvershootC=4.4`, `holdPeakToPeakC=4.1`, `residualHeatAfterHoldEntryC=3.4`

Compared with the previous focused run `thermal-1783565969037-serial-303a-1001-d0-cf-13-08-a1-48`,
that reduced overshoot from `9.5 -> 7.9` at `100°C`, `6.5 -> 4.6` at `180°C`, and
`5.1 -> 4.4` at `220°C`. The remaining blocker is no longer "hold inherited too much power". It is
"predictive coast still starts too late for the hotplate's stored heat", so the next tuning pass
must push `approachLeadTicks`, brake distance, or the near-target floor harder from measured
residual heat.

The next real HIL run with `holdReheatPowerPermille` confirmed that diagnosis. Recovery power became temperature-zone aware and all six targets kept overshoot inside `<=3.0°C`, but the acceptance failures stayed concentrated in `holdPeakToPeakC`: `60=3.4`, `100=3.2`, `140=4.6`, `180=5.7`, `220=3.5`, `250=5.1`. That means the controller is now braking and reheating in the right direction, but the equilibrium hold baseline and hold PI still need another temperature-zone-specific refinement.

The final focused HIL run on the accepted hold-residency metric was `thermal-1783534484939-serial-303a-1001-d0-cf-13-08-a1-48`. With the low-temperature point tuned down to `approach=420 / floor=200 / hold=280 / reheat=340 / kp=22 / blend=1` and the high-temperature point left at `approach=960 / floor=860 / hold=850 / reheat=930 / kp=12 / blend=1`, the measured results were:

- `140°C`: `riseTimeMs=142892`, `maxOvershootC=2.6`, `holdPeakToPeakC=2.9`
- `250°C`: `riseTimeMs=136873`, `maxOvershootC=2.8`, `holdPeakToPeakC=2.9`

That run passed with no validation failures and produced the owner-facing report HTML alongside `run.json` and `samples.ndjson`.

## Power abstraction

The controller should output equivalent heat power, not backend-specific voltage or PWM details.

- PPS backend: map requested power to a `100 mV` aligned CH224Q PPS voltage request and keep the MOS gate static between voltage changes. Suppress sub-`500 mV` request churn. Every actual PPS change unloads the heater; upward steps use a `150 ms` settle window and downward steps use `500 ms`, because loaded request changes and short downstep settling both caused resets on the real device. Mode changes, fixed-PD/current-limit fallback, initial mode establishment, and failure recovery retain the same protection.
- Fixed PD backend: choose the nearest PDO that is not below the equivalent voltage target, then use MOS PWM to synthesize the requested power.
- Current-limit fallback remains a safety boundary. If the available current cannot support the requested PPS voltage, fall back to fixed PD PWM and cap duty by the same current contract.

CH224Q PPS requests use register `0x53` in `100 mV` units. Do not design a PPS hold loop that depends on `20 mV` or AVS `25 mV` steps unless the hardware adapter explicitly supports that path.

## Self-test packet

A useful thermal self-test packet has four files:

- `run.json`: parameters, source identity, target ladder, per-target metrics, validation result, and file paths
- `samples.ndjson`: raw time-series samples with phase, target, source request, explicit source telemetry, explicit heater telemetry, explicit heater parameters, status snapshot, and timestamps
- `thermal-profile.candidate.json`: preview/save-compatible profile proposed by the tooling
- `report.html`: self-contained chart report with per-stage temperature, voltage, and current/output plots

For long-lived comparison and retuning, freeze an accepted baseline bundle instead of relying on a mutable latest-report directory. The frozen bundle should keep `index.html` as the canonical report, add a committed bundle manifest (`run.bundle.json`), keep the accepted preview/save-compatible profile (`thermal-profile.accepted.json`), and copy provenance summaries that tie the report back to the underlying accepted runs. Local MHTML browser snapshots are not a reliable primary delivery format because browser sandboxing can block chart scripts.

The default ladder for Flux Purr now uses sparse coverage:

`60, 140, 220°C`

The supported explicit target ladder remains:

`60, 100, 140, 180, 220, 250°C`

Do not include `300°C` in first-version thermal self-test acceptance, even if it remains a runtime preset.

## Candidate identification

The candidate generator is deliberately not a general parameter search. A stage produces three
classes of evidence, and each class updates only the profile fields that can explain it:

- insufficient near-target heat: derive `holdPowerPermille` from hold median output, then raise
  `approachFloorPowerPermille` and `holdReheatPowerPermille` from the measured sustain gap;
  `approachPowerPermille` and `warmupPowerPermille` are only raised as dependent ceilings
- excess stored heat: increase `brakeDistanceCentiC`, increase approach damping, and increase
  predictive lead without collapsing the profile's sustain floor
- hold ripple: rebase hold power on measured equilibrium and narrow the reheat gap; adjust
  `holdOnCentiC`, `holdOffCentiC`, `holdBlendTicks`, and `holdKpPermillePerC` only for the
  observed hold-entry or reentry behavior

The generated candidate always materializes all resulting runtime values in the profile. Firmware
does not need a replacement build to carry a bench-specific result: the same candidate is previewed
through the control API and saved through the EEPROM-backed profile path after validation.

Development identification uses the sparse `60 / 140 / 220°C` anchors. The profile interpolation
rebuilds intermediate supported points from those anchors; a full supported-ladder run is reserved
for final acceptance rather than repeated during tuning.

Focused tuning does not need to rerun the full ladder every time. The CLI now accepts `--targets-c`
with a comma-separated subset of the supported ladder, for example `140,250`, while still
retaining the full candidate profile and saved-profile flow. That keeps acceptance behavior
stable while making real HIL iteration materially faster.

Real HIL also established two control facts that were easy to miss while only looking at point
values:

- sub-floor PPS gate modulation is useful in `hold`, but weakening `warmup` or `approach` at the
  `5V` hardware floor makes low targets stall below setpoint
- predictive lead must not be allowed to push the controller out of `warmup` before the actual
  temperature error has entered the configured brake band; otherwise low targets collapse into
  single-digit output far too early

The current controller therefore keeps sub-floor gate synthesis scoped to `hold` and uses actual
error, not projected lead error, for the `warmup -> approach` phase transition.

## Validation gates

The applied saved-profile run is acceptable only when every target satisfies:

- maximum overshoot `<= 3.0°C`
- continuous hold peak-to-peak `<= 3.0°C`; the default HIL hold window is `60s`, and once hold sampling starts the full window is counted continuously

Each stage has a default `300s` safety deadline. If the deadline expires or runtime state is lost, the self-test actively sends `heaterEnabled=false`, forces `activeCoolingEnabled=true`, and stops the ladder instead of moving to the next target.

The acceptance floor for host sampling remains `3Hz`, but the accepted full-range 20Hz baseline bundle uses `100ms` host sampling and records actual `intervalMs` plus rolling rate in every sample. Earlier tuning often used `300ms` sampling to leave more margin on the `115200`-baud JSONL path; the committed baseline keeps the denser cadence because the point of that artifact is later comparison, not just minimum acceptance proof. Full `/status` responses are about `1.9KB`; source polling runs independently from the Flux Purr control sampler, each record carries source snapshot age, and source telemetry that does not advance for `2s` is rejected.

The firmware control loop now runs at `20Hz`. Each loop reads `64` RTD ADC conversions, retains the fractional millivolt mean through calibration and PT1000 conversion, and then applies the profile-controlled temperature filter. This is roughly `1280` ADC conversions per second and removes the old integer-millivolt temperature steps while preserving the accepted floating-point measurement path through runtime status. With this oversampling in place, the default and tuned `tempFilterAlphaPermille` is `700`; the faster filter avoids turning measurement lag into a false full-speed-to-stable failure while remaining API/EEPROM adjustable. The front-panel rendering remains quantized to deci-Celsius, but the HIL and status paths keep the centi-Celsius telemetry.

Static brake distance is not sufficient when heater assembly or thermal mass changes, so warmup handoff combines point-level brake distance with confirmed filtered rise rate times `approachLeadTicks`, capped below the `warmupReenterCentiC` boundary. Predictive expansion is accepted only when both actual and filtered temperatures have reached the expanded boundary; entering the ordinary static brake still follows actual temperature so filter lag cannot block normal handoff. This reuses API/EEPROM profile parameters instead of adding a hidden firmware tuning constant.

Raw RTD samples must remain distinguishable from thermal momentum in the report, but temperature motion itself is not a sensor fault. Every valid RTD batch independently runs the over-temperature check, while the control loop, front panel, and runtime status use the median of the most recent three valid batch temperatures. This rejects a single batch-level ADC shift without suppressing a sustained thermal change; any open, short, ADC read failure, or over-temperature condition immediately clears that short history and retains the existing heater shutdown path.

Approach and hold tuning must remain independent. A pre-hold acquisition failure may adjust approach power, floor, tail window, brake distance, damping, and lead, but it must not raise hold baseline or reheat power without hold samples. Likewise, `holdReheatPowerPermille >= holdPowerPermille` and `approachPowerPermille >= approachFloorPowerPermille` are valid local invariants; coupling `holdReheatPowerPermille` to `approachFloorPowerPermille` causes low-temperature tuning to oscillate between overshoot and underpowered acquisition. When high early approach floor is required but that same floor creates a near-target residual-heat tail, `approachTailWindowCentiC` linearly tapers only the approach floor down to the existing `max(holdReheatPowerPermille, holdPowerPermille)` floor over its final error window. A zero window is the legacy behavior, so the new parameter cannot silently reshape already-saved points.

Failures should report the target temperature and raw samples. The current tooling runs the ladder in `thermalControlProfile.op=preview`, updates the candidate after each tuning stage, and only writes the final tuned profile through `thermalControlProfile.op=save` after the whole ladder passes. IsolaPurr LAN reads and writes should use bounded retry with readback so a single transient timeout does not invalidate an otherwise complete HIL ladder.

The self-test records full-speed-to-stable separately from the one-minute hold window. The timer
starts at the first firmware transition out of `warmup`; within `10s`, a continuous window must
start in which firmware remains in `hold` and measured temperature remains within `±1.5°C` for
`10s`. The stage then continues for the complete `60s` hold window, where maximum overshoot and
peak-to-peak must each remain `<=3.0°C`. Any runtime reset, heater disarm, target/mode mismatch, or
source fault remains terminal.

The accepted sparse HIL anchors on the authorized source are:

- `60°C`: `thermal-1783699709933-serial-303a-1001-d0-cf-13-08-a1-48`, settle `7.534s`, overshoot `1.2°C`, full-`60s` hold peak-to-peak `2.3°C`
- `140°C`: `thermal-1783700052289-serial-303a-1001-d0-cf-13-08-a1-48`, settle `8.661s`, overshoot `0.4°C`, full-`60s` hold peak-to-peak `2.3°C`
- `220°C`: `thermal-1783702036504-serial-303a-1001-d0-cf-13-08-a1-48`, settle `0.905s`, overshoot `1.1°C`, full-`60s` hold peak-to-peak `2.6°C`, mean host sampling rate `3.33Hz`

## Accepted full-range baseline bundle

For the `56mm x 56mm`, `3.2Ω` heater plate on the PD PPS `3A` class hardware path (`5-21V`, nominal `60-65W` envelope), the frozen comparison artifact is:

`thermal-self-test-runs/baselines/56x56mm-3p2ohm-pd63w-pps3a/accepted-full-range-20hz/`

Use this bundle as the default regression and retuning reference only for the same hardware class. It contains the canonical interactive report (`index.html`), a committed manifest (`run.bundle.json`), aggregated raw samples (`samples.ndjson`), the accepted preview/save-compatible profile (`thermal-profile.accepted.json`), and copied provenance summaries for the underlying accepted runs.

The validation coverage in this bundle is `60 / 80 / 100 / 120 / 140 / 160 / 180 / 220 / 240°C`. The accepted anchor profile inside the bundle remains `60 / 100 / 140 / 180 / 220 / 250°C`, so dense intermediate targets continue to be verified through the same interpolation rule that firmware uses at runtime.

The authorized source remains IsolaPurr `856a141cdbd4` at `http://192.168.31.122` in `auto_follow` mode. In-run telemetry stayed inside the same `5-21V PPS / 3A` class, reaching `20.5V` contract, about `20.45V` measured source voltage, about `2.995A` measured source current, and about `58.1W` peak measured source power. Treat that artifact as the accepted `60-65W` PPS calibration envelope for this hardware path even though the run does not saturate the full advertised source envelope at every target.

For a source that advertises a `5V` PPS minimum, down-ramp power accounting must use the active
PPS request rather than only the requested floor. The PPS request is intentionally rate-limited,
so a controller that has reduced its requested heat power can otherwise leave the MOS continuously
open while voltage still walks down from the previous high request. In approach, static `50ms`
`0%/100%` pulse-density gate ticks preserve the requested average power across that transition.
On the authorized `856a141cdbd4` source, the `60°C` profile point `brake=850`,
`approach=500/floor=170`, `hold=135/reheat=170`, `lead=5/6`, and a `5000mV` working floor passed
two complete 60-second HIL holds: settle times were `9.600s` and `8.902s`, maximum overshoot was
`1.13°C` and `1.49°C`, and hold peak-to-peak was `2.58°C` and `2.71°C`.

The high-temperature result required a separate saturated-zone rule. When measured near-target output is already at least `90%`, slope remains at or below `1°C/s`, and hold is not reached, the tuner moves warmup/approach to the stable-band edge and generates a near-saturated hold baseline. If a saturated hold then oscillates on both sides of target, the tuner widens the existing `holdOffCentiC..overshootCutoffCentiC` taper from measured amplitude instead of cutting PPS voltage nearly to zero. At `220°C`, widening `overshootCutoffCentiC` from `180` to `383` reduced the observed hold range from `3.2°C` to `2.6°C` by removing the `~99% -> 22% -> 100%` power cycle.

The final candidate was saved through `thermalControlProfile.op=save`. After the MCU restarted on the same authorized port, status returned `profileSource=saved`, preview disabled, `tempFilterAlphaPermille=700`, `autoAdjustableWorkingFloorMv=6100`, and the saved `220°C` point unchanged. Parameter replacement therefore uses API plus EEPROM rather than a firmware rebuild.

For parameter sweeps, repeat `--candidate-profile-file` at one target. The batch holds one Flux
lease and one IsolaPurr source session across all candidates, writes separate samples and charts for
each candidate, never saves EEPROM, and starts the next candidate when temperature is at or below
`max(40°C, target-30°C)`. Source setup must happen before the Flux readiness handshake; a port node
alone is not sufficient evidence of readiness, so the handshake requires a real heater-off status.

Historical run `thermal-1783536535189-serial-303a-1001-d0-cf-13-08-a1-48` used IsolaPurr LAN source `http://192.168.31.224`, which resolves to device `f293cc9c139e` rather than the authorized source `856a141cdbd4`. Its thermal values are retained for diagnosis only and do not constitute HIL acceptance:

- `60°C`: `riseTimeMs=36339`, `maxOvershootC=1.8`, `holdPeakToPeakC=2.4`
- `140°C`: `riseTimeMs=198330`, `maxOvershootC=2.6`, `holdPeakToPeakC=2.9`
- `250°C`: `riseTimeMs=211342`, `maxOvershootC=2.1`, `holdPeakToPeakC=2.2`

That run exercised profile persistence, but it cannot prove thermal performance for the authorized bench source.

The current focused `220°C` development reruns on `2026-07-09` changed the picture again.
Those reruns exposed two concrete truths that matter more than the older sparse-ladder pass:

- the old runtime temperature spike filter could latch temperature indefinitely while heating;
  real HIL showed `rtdRawAdcMv` continuing to rise while `currentTempC` stayed frozen. That filter
  is not part of the runtime control path: valid RTD samples now pass directly to the controller,
  and raw telemetry remains available for diagnosis.
- real flash verification uses `local-esp32s3-release`, which resolves to the workspace build at
  `target/xtensa-esp32s3-none-elf/release/flux-purr`. A legacy `firmware/target/...` build is
  exposed only as `local-esp32s3-release-firmware-target`, so a stale nested target cannot silently
  replace the current workspace artifact.

With the corrected artifact flashed, focused real HIL at `220°C` still did not clear acceptance:

- `thermal-1783608810927-serial-303a-1001-d0-cf-13-08-a1-48`:
  `riseTimeMs=64496`, `maxOvershootC=1.0`, `holdPeakToPeakC=3.4`
- `thermal-1783609087433-serial-303a-1001-d0-cf-13-08-a1-48` after raising `holdPower`,
  `holdReheat`, and tightening `holdExit`: `maxOvershootC=2.3`, `holdPeakToPeakC=4.7`
- `thermal-1783609283479-serial-303a-1001-d0-cf-13-08-a1-48` after reverting to a narrower
  reheat-only change: `maxOvershootC=1.0`, `holdPeakToPeakC=3.4`

Another candidate that only removed `220°C` `approachLeadTicks` (`thermal-1783609472418-...`)
never reached hold at all: it plateaued near `132.3°C` for more than two minutes while
`heaterOutputPercent=98`, `heaterPhysicalOutputPercent=100`, and `pdContractMv≈17500`. That is not
an acceptance run; it is evidence that the current high-temperature power-request mapping can still
fall into an underpowered plateau for some near-target candidates.

So the current truth is narrow and useful:

- the temperature-sample latch bug is fixed
- the controller can now reliably climb back into the `220°C` neighborhood on the corrected
  firmware image
- the present hybrid controller is still not acceptance-ready at `220°C` because hold ripple stays
  around `3.4°C` on the best focused reruns, and some candidate variants can still collapse into an
  underpowered mid-temperature plateau

Historical focused `220°C` work replayed the same saved seed profile across three firmware
revisions with IsolaPurr LAN source `http://192.168.31.224`. Because that endpoint belongs to the
wrong source device, these runs are diagnostic history rather than acceptance evidence:

- `thermal-1783621074146-serial-303a-1001-d0-cf-13-08-a1-48`: the first hold-asymmetric baseline
  revision proved that using `holdReheatPowerPermille` as a real under-target hold baseline can
  eliminate the old zero-output valley behavior, but it also exposed a new problem at hold entry.
  Because filtered temperature lag was still under target when actual temperature had already
  crossed above target, that first cut entered hold too hot and regressed to
  `maxOvershootC=2.3`, `holdPeakToPeakC=5.4`, `stopReason=timeout`.
- `thermal-1783621527243-serial-303a-1001-d0-cf-13-08-a1-48`: after restricting that reheat bias
  to cases where actual temperature is still under target at hold entry, the same seed profile
  improved to `firstHoldTempC=220.3`, `maxOvershootC=1.7`, `holdPeakToPeakC=4.8`, and completed
  the full one-minute hold window.
- `thermal-1783622131711-serial-303a-1001-d0-cf-13-08-a1-48`: sample replay then showed the
  remaining low-side dip was caused by staying in `hold` while actual error had already widened far
  past the reentry threshold but filtered error was still lagging. After making `Hold -> Approach`
  reentry trust actual error only when the dip is materially deeper than the hold band, the same
  seed profile reached `maxOvershootC=1.0`, `holdMaxBelowTargetC=2.4`, and
  `holdPeakToPeakC=3.4`.

That sequence matters. At `220°C`, the controller is no longer failing for one vague reason. The
latest evidence says:

- the structural entry/exit problems in hold are mostly corrected
- overshoot is already inside the acceptance limit with the unchanged seed profile
- the remaining gap is now a narrow low-side hold ripple window, not a broad instability pattern

In other words, current evidence no longer says “the algorithm is still structurally wrong at
`220°C`”. It says “the revised control law is directionally sound, and the remaining work is to
finish the `220°C` hold-window parameter fit.”

The later authorized-source run `thermal-batch-1783684057089-serial-303a-1001-d0-cf-13-08-a1-48`
showed why equilibrium power and residual heat must be classified separately. Raising the `220°C`
hold path from roughly `62 / 70%` to `70 / 78%` reduced the low-side drop from `4.5°C` to `2.4°C`,
but the plate still entered hold with a strong positive slope and reached `+3.0°C`; the complete
60-second peak-to-peak remained `5.4°C`. A candidate generator must classify this shape as
residual-heat dominant, advance `brakeDistance / approachLead`, and rebase hold power from measured
equilibrium. Treating it as ordinary hold ripple and raising hold power makes the two sides fight.

The full-speed-to-stable gate must use the actual specification: the stable window starts no later
than 10 seconds after leaving warmup, remains in firmware `hold`, stays within `±1.5°C`, and lasts
10 continuous seconds. Replaying existing samples with that gate leaves `140°C` as a valid pass,
while the earlier `60°C` result is no longer a pass despite its acceptable one-minute peak-to-peak.

If a self-test process is terminated externally instead of reaching its normal cleanup path, the
native USB lease may remain owned until `flux-purr-devd` is restarted. Focused HIL iteration
should therefore prefer target subsets over killing long full-ladder runs mid-flight.

## Hardware boundary

Flux Purr self-test uses the repo-local `flux-purr` CLI through `flux-purr-devd` for the device under test. The host self-test path abstracts a bench source provider, with `isolapurr` as the current default and validated provider. IsolaPurr remains an external PD source and must be prepared through released `isolapurr` / `isolapurr-devd` tools, not source commands or raw local HTTP. Thermal HIL configures `65W`, enables PD Fixed and PPS, selects `auto_follow`, and checks one live USB-C reading above `5V` before testing. It does not manually control TPS, force a voltage, replug the source port, or test IsolaPurr behavior; Flux Purr owns subsequent PD/PPS requests.

### Recovering a source stuck at 5V

Capability readback does not prove that the live USB-C output is following the sink. A source may
correctly report `65W` or `100W`, PD Fixed and PPS enabled, and `auto_follow`, while its live USB-C
telemetry remains near `5V` after Flux Purr requests a higher PPS voltage. Do not classify thermal
results or tune a profile in that state. The source contract is stale and must be recovered first.

Use an IsolaPurr runtime output restart when any of these conditions holds:

- Flux Purr requests more than `5V`, but IsolaPurr USB-C telemetry remains near `5V`.
- Flux Purr `pdRequestMv` / `pdContractMv`, IsolaPurr USB-C voltage, and Flux Purr VIN do not
  converge after the normal negotiation interval.
- A `65W` / `100W` capability change reads back successfully but the live output retains the old
  contract.
- Flux Purr resets during a failed negotiation and the recovered source remains at `5V`.

The recovery sequence is:

1. Stop the run and confirm Flux Purr reports `heaterEnabled=false` and
   `heaterPhysicalOutputPercent=0`.
2. Confirm the intended IsolaPurr capability and `auto_follow` configuration are already saved.
3. Disable the IsolaPurr runtime output, wait about one second, then enable it again:

   ```bash
   isolapurr --json power runtime output \
     --url http://192.168.31.122 \
     --enabled false

   sleep 1

   isolapurr --json power runtime output \
     --url http://192.168.31.122 \
     --enabled true
   ```

4. Reassert automatic USB-C request tracking:

   ```bash
   isolapurr --json power output auto \
     --url http://192.168.31.122
   ```

5. Wait for Flux Purr to boot on the same owner-authorized serial path. If the path changes or
   disappears, stop; do not select another port automatically.
   IsolaPurr CLI target selectors are mutually exclusive: use the explicit `--url` shown above or
   `--device-id`, never both in one command.
6. Before arming the heater, read both devices and require all three voltage views to agree within
   the expected measurement tolerance: Flux Purr `pdRequestMv` / `pdContractMv`, IsolaPurr live
   USB-C `voltage_mv`, and Flux Purr VIN `voltageMv`. When the requested contract is above `5V`, a
   single healthy `>5V` reading is necessary but not sufficient; the readings must remain current
   and follow the requested contract.

This restart is an exception for a stuck or stale source contract. Do not cycle output for ordinary
PPS steps, between healthy thermal stages, or as a way to hide a repeatable source or sink fault.
Never cycle it while the heater is armed. A failed post-restart readback is an infrastructure
failure, not thermal-profile evidence, and the run must remain unsaved.

### Rejecting RTD samples during PPS transitions

High-current PPS ramps can couple into the RTD ADC path even when the source contract and heater
remain stable. The identifying trace is a physically impossible temperature staircase synchronized
with each `500mV` PPS request step, such as a repeated `0.5°C` decrease every `100ms` while the
heater is still delivering power. This is measurement interference, not plate cooling and not a
thermal-profile response. Tightening the sensor connection can reduce it, but HIL must not assume
the wiring change alone removes every transition transient.

Firmware keeps the last trusted control temperature while the PPS request is changing. After the
request has remained unchanged for `300ms`, it clears the temporal RTD window, seeds both the RTD
estimate and controller filter from the new stable measurement, and resumes slope prediction. The
raw sample still runs short, open, ADC-read, and over-temperature checks on every control cycle;
the transition guard must never delay those safety paths. Re-seeding only the RTD median is
insufficient because the controller's previous filtered value would turn the first stable sample
into an impossible slope and trigger a false warmup handoff.

Parameter tuning is valid only when the sample trace shows all of the following:

- PPS request steps do not produce an opposite-direction temperature staircase.
- `heaterFilteredSlopeCPerS` remains physically plausible when the request settles.
- warmup, approach, and hold transitions follow the plate temperature rather than a voltage step.
- the run starts below the selected cooldown threshold, so repeated focused trials do not compare
  different heat-soak states.

For the `56x56mm / 3.2ohm` plate, focused `pps5a` 60°C tuning uses a `6100mV` working floor,
`11.0°C` brake distance, `85%` warmup power, `14%` approach floor, and six approach lead ticks.
Run `thermal-1784007933933-serial-303a-1001-d0-cf-13-08-a1-48` resolved `100w -> pps5a`,
completed the full hold, settled in `7.499s`, reached `1.35°C` maximum overshoot and `2.45°C`
hold peak-to-peak, and measured a source peak of approximately `18.964V / 3.810A / 72.258W`.
This single-target run is tuning evidence only. It must not be copied into `baselines/**`, renamed
to `index.html` or `run.bundle.json`, or presented as a canonical report. A 100W baseline may be
frozen only after the accepted full-range source runs are aggregated with the same manifest,
provenance, hardware, cadence, and report-format fields used by the existing 65W canonical bundle.
The saved EEPROM readback must report `profileSource=saved`, `thermalProfileMode=100w`, and
`thermalProfileResolvedBank=pps5a` before the focused point is reused as a tuning seed.

A repeatable drop to `5V` at the first low-power hold correction is a profile problem rather than
a stale source contract. Check the active saved bank and the seed preset before repeating the run.
For the accepted 65W heater plate, `autoAdjustableWorkingFloorMv` is `6100`; saving a stale
`5000mV` seed overwrites that EEPROM guardrail and allows the controller to request `5V` during
hold. The observed sequence is a healthy `6V` contract, a low-duty hold correction, a `5V`
request, and then `core_usb_uart` reset while the source remains powered. Restore the committed
`6100mV` preset to `pps3a`, restart IsolaPurr output once with the heater off, and verify a direct
heating run keeps the minimum contract above `6V` before resuming thermal acceptance.

When an explicit `manual-forced` source mode is used, source preparation and Flux Purr lease acquisition form one rollback boundary. If the target never becomes ready or lease acquisition fails after the source was forced on, the CLI must restore IsolaPurr output to `auto` before returning the error; batch and single-run paths share this behavior.

## Source and profile banks

Thermal tuning has two fixed persistence banks: `pps3a` for the 65W path and `pps5a` for the 100W path. The host-facing choice is `auto|65w|100w`; only `auto` resolves from advertised PPS capability (`20V` coverage and at least `5A` select `pps5a`). Explicit choices are diagnostic and tuning intent, not a source-mismatch interlock, so reports must preserve both selected mode and detected capability class.

The 65W source preset remains `20V / 3.25A`; this preserves accepted tuning semantics. The 100W preset is `21V / 5A`, with PPS and PD Fixed enabled. `preview` is always a single RAM overlay. A successful save or clear must contain the destination bank, and each frozen baseline bundle remains the committed seed for that bank.

For banana-jack bench output, keep the IsolaPurr USB-C VBUS path disconnected unless the operator explicitly chooses shared USB-C output.

## Source-current headroom

Do not treat the advertised PPS current as entirely available to the heater. The Flux Purr board,
conversion loss, cable resistance, and source current-limit quantization share the same budget. A
real 60°C run reset while CH224Q reported `3.25A`, IsolaPurr TPS read back `3.20A`, and live load was
already around `3.15~3.17A`. The source had no latched fault. The heater voltage ceiling therefore
uses `min(capability current, live current) - heaterCurrentReserveMa`; the reserve is part of the
EEPROM/API thermal profile settings so a new DIY build can tune it without a firmware replacement.
