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
