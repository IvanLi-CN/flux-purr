# Firmware tuning remediation audit

## Provenance

The strategy baseline is `d86a4eddccfc9d31cbe9b9cb2f2d597b8b525278`. Its external `cli/thermal_workflow.rs` and the remediation starting commit `240549cbc2427c250e687dd9c900f360a0d24b0e` have identical blob `43371a8a360932e325f26152eed1d6c091860228`. No abandoned implementation, checkout or evidence is an input.

| Concern | Frozen external baseline versus current external runner | Firmware boundary / disposition |
| --- | --- | --- |
| Seed, interpolation, candidate base transform, eleven-component score | Identical workflow source | Independent device implementation; captured-prefix comparisons cover the shared ordinary paths, not every possible vector |
| Evidence-specific candidate selection | The current external runner excludes `missed_lower_band_before_limit` from the conservative high-side branch; the frozen runner did not | Intentional R2 correction: a measured lower-side miss uses the lower-side branch. This is a declared difference, not frozen all-vector parity |
| Default nine targets, recursive order, endpoint blocking, retries, accepted-point freeze | Unchanged | Existing device regressions cover order, bounds, budget and blocking |
| Sampling and stage enforcement | External scout collects beyond the original stage deadline; confirmation enforces it | Device ordinary trials stop at the deadline. Prefix comparisons use only samples recorded before that stop; a historical tail cannot prove an already stopped device passed |
| Conditional extension | No automatic exception in the frozen runner | Approved R3/R4 device-only exception; same candidate and clock, bounded and audited. The external engine is unchanged |
| External telemetry / reports | Source client and four-file output remain external | Device-local facts and five-file v2 report, explicit unavailable external measurements |

The current-branch differential tests are not evidence of universal equality with the frozen commit. They compare ordinary same-input paths and explicitly identify historical prefixes that now enter the approved exception. An incomplete old recording never receives synthesized extension or confirmation samples.

## Complete 60°C trial review

Source: this workspace's `firmware/target/hil-return-original-bundle/`, run `firmware-pps5a-1791035605955`, complete sequence `0..4572`. No profile was saved. All nine 60°C trials were rejected; the 240°C endpoint passed scout and confirmation, while dependent interior targets were blocked.

| Trial | Approach floor / ceiling / Hold base (‰) | Brake (centi-°C) | Overshoot / qualified Hold p2p (°C) | Finding |
| --- | --- | --- | --- | --- |
| 0 | 160 / 450 / 135 | 1100 | 4.10 / 4.15 | High-side failure; floor still above allowed Hold-derived minimum |
| 1 | 160 / 450 / 135 | 1100 | 4.57 / 4.84 | Same high-side failure |
| 2 | 100 / 350 / 100 | 1400 | 1.53 / 1.92 | At allowed floor; first window interrupted; original timing failed |
| 3 | 100 / 350 / 100 | 1400 | 1.56 / 1.83 | Same timing failure |
| 4 | 100 / 350 / 100 | 1400 | 1.51 / 1.73 | Same timing failure |
| 5 | 100 / 250 / 100 | 1700 | 0 / unavailable | Original deadline ended capture before qualified Hold; missing ripple is not zero |
| 6 | 100 / 350 / 100 | 1400 | 1.51 / 1.97 | Same timing failure, not the sole input to the decision |
| 7 | 100 / 350 / 100 | 1400 | 1.52 / 1.94 | Same timing failure |
| 8 | 100 / 250 / 100 | 1700 | 0 / unavailable | Original deadline ended capture before qualified Hold |

Several floor candidates remained within the overshoot/ripple bounds but broke their first window 13.9–15.8 seconds after leaving Warmup. That observation supports a bounded recovery experiment, not a claim that hardware capability is exhausted or that an unrecorded confirmation would pass.

## Timing policy

Policy version `1` binds both limits and the candidate-derived floor before a trial. The original stable-window-start deadline remains 10 seconds at ≤150°C and 5 seconds above it. An eligible exception allows the window to start within 30 or 15 seconds respectively, measured from the same first Warmup exit. A window started inside the active deadline may complete its required ten seconds afterward. Whole-trial timeout remains 180 seconds and the original target budget remains twenty minutes.

Eligibility requires configured `max(approachFloor, holdBase) == max(100, holdBase)` and a measured Approach logical request at that effective floor, including the production controller's whole-percent rounding. A prior floor touch followed by a higher Approach request loses eligibility; protective zero output is not floor evidence. Hold phase is not qualified Hold. No parameter, phase, Hold-entry threshold or clock changes when the exception activates.

The stable window needs at least 3 Hz over its actual duration; repeated timestamps do not count as new samples and a gap greater than one second invalidates an unverified window. Overshoot/ripple failure, safety faults, trace backpressure and cancellation cannot be excused by extension. A scout still needs twelve seconds of qualified Hold and a separate unchanged-candidate confirmation still needs sixty seconds. EEPROM writes remain explicitly confirmed and identity-bound.

The cap is an implementation parameter under R3, pending HIL validation. Software regressions exercise prerequisites, cap exhaustion, dense stable windows, full confirmation, thermal failure and sensor failure. Reports retain original deadline failure, floor facts and extension disposition; old records remain unavailable rather than being backfilled.

## Unverified-window search consistency

Authorized default-scope run `firmware-pps5a-1791050858694` on firmware `0.27.6-dev.74d2bd0` independently confirmed 60, 240, 140 and 100°C before reaching 80°C. At 80°C, scout trials 0 and 1 restarted a broken stable window at 18,252 and 20,838 milliseconds after Warmup, within their eligible 30-second cap. Neither completed the required ten-second verification. Their overshoot / qualified-Hold ripple were 1.55 / 1.82°C and 1.76 / 2.23°C respectively. A timely window start is insufficient for qualification.

Four evidence defects caused candidate search to diverge from that rejected qualification: its `full_speed_failed` flag omitted missing window verification; the failure classifier treated a timely restarted window as complete; the classifier's band/deadline-temperature projection remained fixed at the original ten-second deadline after a valid extension; and ranking allowed an unverified window to receive completed-stage and passing-metric scores. The frozen external runner counts incomplete stages or a stability failure as full-speed failure. The firmware projection therefore retains failure until verification, ranks the stage as incomplete, classifies actual band breaks under the active bound deadline, and resets only the nearest-deadline-temperature cache when extension first activates. Earlier entry/break facts, original failure, candidate identity and clocks remain intact. Production control and all qualification thresholds remain unchanged.

The complete original run was safely canceled after identifying the mismatch: sequence `0..8400`, device digest `5f985149bb065de3c83dbc75e9ea060d37dfd44fdd8d91b4a730589e2d85f759`, verified five-file report under `firmware/target/hil-full-74d2-retry-bundle/`. The terminal record is failed/canceled, not default qualification. Post-cancel readback showed requested/final heater output zero, RAM preview cleared, and confirmed PPS 12V/5A. No profile was saved. The regression fixture preserves both actual 80°C traces and their device decisions without appending samples. Five targeted regressions reproduce the defects before correction and pass afterward; complete default-scope qualification requires fresh exact-artifact HIL.

## Scout observation completeness

Authorized full-scope run `firmware-pps5a-1791085013090` on firmware `0.27.6-dev.4486a39` independently confirmed 60, 240, 140, 100, 80, 120 and 180°C. The 80°C prediction reduced Approach ceiling from 234 to 180‰, Hold base from 150 to 100‰, and increased brake distance from 1828 to 2128 centi-°C; its independent confirmation recorded zero overshoot and 0.71°C qualified-Hold ripple. This validates forward progress of the corrected search, not the incomplete nine-target qualification.

At 160°C, trial 7 left Warmup at `3,370,743ms` and restarted its stable window at `3,385,419ms`, 14,676ms later and inside its observed-floor 15-second cap. The trial ended at `3,392,366ms`: only 6,947ms of that window had been observed, while qualified Hold had reached its twelve-second minimum. The runtime used that minimum duration as an unconditional collection cutoff. This conflicts with the already-bound rule that a timely window may complete its ten seconds after the start deadline.

Collection therefore ends successfully only when both the required Hold duration and stable verification are present. The same candidate and clock remain active during that observation; this is neither a new window-start allowance nor a candidate adjustment. Broken windows after the active cap, thermal failures, invalid measurements, cancellation, event retention failure, the 180-second trial timeout and the twenty-minute target budget still stop or reject the trial. Ordinary thermally invalid trials retain their original minimum-Hold rejection boundary. The external runner's collection behavior is unchanged; the device-side completion correction follows R4 and cannot supply missing historical samples.

The run was safely canceled, preserving complete sequence `0..11254`, device digest `39566c70491f36a54ffedf41371df1348450b89c9c5e5b78c081795992cf27e0`, verified five-file digests and merged trace digest `sha256:c2a8ec94e3ef9a946ec5f503218b4255d85f3469476348b07248a9ec33999e58`. All 4,692 cooldown samples record zero heater request and MOS output. Post-cancel readback showed idle, zero heater output, cleared preview, full-fan post-heat cooling and confirmed 12V/5A PPS. The standard report remains under `firmware/target/hil-full-4486-bundle/` and correctly concludes failed/canceled. No candidate was saved; 160 and 220°C remain unqualified in this record.

## Recorded sample rate and independent confirmation

Firmware `0.27.6-dev.a0cd587` completed diagnostic run `firmware-pps5a-1791090192820` at the original twenty-minute 160°C budget. Four short trials passed, including a timely window verified over 10,330ms in scout 0; four independent confirmations failed, and no candidate was adopted or saved. The complete sequence `0..3904`, device digest `00d8313e114c356197d14942889a80483b7dbe1e14d3ebe4f66e241b9c2751db` and five-file report under `firmware/target/hil-160c-a0cd-bundle/` retain that failed result. All 1,405 cooldown samples have zero requested/final heater output.

Default-scope run `firmware-pps5a-1791091457042` on the same artifact independently confirmed 60°C (60,238ms qualified Hold, 0.90°C overshoot, 1.39°C ripple) and 240°C (60,088ms, 0.91°C, 2.10°C). At 140°C, scout 0 started a timely stable window at `910,853ms` but only verified at `992,851ms`: 247 samples over 81,998ms just reached 3.000073Hz. Its first twelve seconds contained 36 samples spanning 11,714ms, only 2.987878Hz. The recorder waited at least 300ms before observing the next production cycle, so nominal cadence did not reliably meet the required actual rate. A 200ms minimum interval adds scheduling margin; actual timestamps and counts, duplicate/gap handling, thermal gates, deadlines and budgets remain authoritative.

That scout had 96,620ms of qualified Hold. The device mistakenly emitted `holdConfirm=true`, `adopted` and `freeze=true` merely because duration exceeded sixty seconds, while its actual state machine still queued an independent confirmation. The host and Web report projections also trusted that marker. These are implementation/evidence defects under R4/R5, not permission to replace independent confirmation with a long scout. Confirmation facts now require `hold_confirm` purpose; report projections require matching trial/decision purpose and all adopted gates. They also retain `targetC`, which is necessary to validate candidate-bound timing before saving. The incorrect original ledger is preserved; it must not qualify a target or a save when reviewed.

The default run was safely canceled after this mismatch was identified. Its failed/canceled standard report remains under `firmware/target/hil-full-a0cd-bundle/`, with complete sequence `0..3470`, device digest `4bde3b87904350a6075603220468a2999e47962b53cae807cbe6998616535792`, verified five-file digests and merged trace digest `sha256:2921201c723f26ce106941e61ca264da8419772ad4a3b657b8d9d54aceec88f2`. All 1,104 cooldown samples have zero requested/final heater output. Post-cancel status showed idle, cleared preview, zero MOS and 12V/5A PPS; no profile was saved. Only the two independent endpoint confirmations are valid per-target evidence, and the incomplete run is not nine-target qualification.
