# 5A production profile conformance audit

## Conclusion

The 5A production controller cannot be declared fully conformant. Source inspection and focused host regressions establish two candidate-materialization defects and two coast-gate deviations. Candidate-materialization fixes and owner-approved coast corrections are implemented; exact-artifact HIL and the overshoot-wording conflict remain open. Neither a short passing trial nor an individual target confirmation establishes complete nine-target qualification.

## Requirement mapping

| Requirement | Implementation and verification | Result |
| --- | --- | --- |
| Normal 5A heating and tuning share the production profile controller | `update_production_at` selects the profile path for verified 5A; production-path tests cover source selection and disarm on source change | Matches the documented controller selection |
| Warmup logical output is 100%; physical soft start is 100ms | The production output path keeps logical and physical output distinct; focused soft-start tests cover the global setting | Matches the checked software behavior |
| Warmup braking combines static distance and filtered slope times point-local lead | `warmup_handoff_error_c` uses their maximum within the re-entry boundary | Formula matches; explicit zero lead was incorrectly materialized |
| Physical Hold entry cannot be widened by a candidate or timeout | `hold_entry_ready` requires actual error within −3..+0.5°C; all Approach-to-Hold branches use it | Matches the checked software behavior |
| Approach predictive coast requires actual and filtered error within `holdExit` | The former `max(holdEntry, holdExit) + 1°C` actual-error allowance is removed | Reproduced regression corrected; HIL pending |
| Coast release requires raw cooling, negative filtered slope, and both raw and filtered low-temperature error | `update_coast_state` checks filtered low error as well as the existing guards | Reproduced regression corrected; HIL pending |
| Candidate identity describes effective production parameters | Generated damping can exceed the production limit, and explicit zero leads inherited global values | Materialization fixes implemented; fresh exact-artifact HIL pending |
| Timing policy 2, stable observation and independent confirmation | Focused runtime regressions exercise fixed 100‰ eligibility, original/cap deadlines, actual sampling, thermal gates and separate confirmation | Software checks pass; full hardware qualification remains incomplete |

## Reproduced defects and disposition

1. In run `firmware-pps5a-1791181537209`, a 240°C predicted candidate specified `approachLeadTicks=0`, while mid-trial production readback reported 7. In run `firmware-pps5a-1791179772746`, generated 60°C candidates specified damping 4050‰, while the effective production value was 4000‰. Point-local explicit zero leads survive RAM and EEPROM materialization. Missing fields in legacy EEPROM layouts still inherit settings during decoding. Generated/seeded profiles are normalized before hashing; preview receipts reject a profile whose normalized identity differs from the supplied candidate identity.
2. A controller regression at target 140°C uses `holdExit=1°C`, actual temperature 138.5°C and filtered temperature within the exit band. Projection crossing the target produces zero Approach output even though actual error is 1.5°C. This violates the literal actual-error gate in `SPEC.md`; it is independent of report rendering.
3. A controller regression at target 60°C uses `holdOn=0.3°C`, negative filtered slope and two raw cooling observations at 59.6/59.4°C. The filtered temperature remains above 59.7°C, yet the controller releases coast. This violates the documented four-condition release gate; it is independent of tuning metrics.

The two coast regressions failed against the production controller before correction and are retained in `runtime_tests.rs` as `heater_approach_coast_requires_actual_hold_exit_error` and `heater_coast_release_requires_filtered_low_error`. The owner-approved correction removes the extra actual-error allowance and adds the missing filtered-error check. It keeps the existing raw-drop, slope and two-observation checks. These source-level deviations are proven; their contribution to a particular HIL rejection has not been isolated.

Read-only comparison with production compatibility anchor `ff4bf3876f548bc4ba6ac1e2cb87259f0d132422` finds the same behaviors in its monolithic `firmware/src/bin/flux_purr.rs`: lines 2518–2522 widen the actual coast gate using Hold entry and twice the measurement margin; lines 2660–2669 omit filtered low error from release; lines 2733–2744 taper mild overshoot. No anchor code or artifact was imported or executed. The approved coast correction intentionally differs from that anchor by requiring actual error within `holdExit` and adding filtered low error to release. These inherited rule/document discrepancies are distinct from the live candidate-materialization defects.

## Model boundary

The current specification assigns thermal-balance/loss-feedforward predictive PI to 3A. Verified 5A uses point-local profile control with live filtered slope and brake/lead parameters. A valid calibrated thermal model remains mandatory for heating and electrical safety, but its natural-cooling fit is not an input to the 5A profile controller's braking calculation. That absence is established by code inspection, not by a document prohibition. A method for deriving a 5A braking seed from the calibrated fit is not defined or implemented by the current contract.

The static-distance/filtered-slope-times-lead rule first appears in commit `9ba38aeaf08dde6719df851f397e7dfa2b88cad5` (`docs: record dense thermal HIL contract`, 2026-07-11 15:45:08 +08:00), in the predecessor path `docs/specs/q2aw6-heater-pid-frontpanel-runtime/SPEC.md`. Commit `505d4c98ddc0f6c8c03144ee9c362641eaaf2a19` (`docs(thermal): lock firmware tuning contract and controller boundary`, 2026-09-26 16:21:54 +08:00) adds the normal/tuning 5A profile-path requirement and [ADR 0015](../../adr/0015-source-class-thermal-controller-selection.md). These dates describe separate decisions; the braking formula predates firmware-owned tuning.

## Overshoot wording conflict

`SPEC.md` requires immediate zero output on overshoot, while `hold_requested_permille` intentionally tapers output between `holdOff` and `overshootCutoff`. The existing regression `heater_hold_softens_mild_overshoot_instead_of_hard_cutoff` confirms nonzero output at 140.9°C for a 140°C target, with `holdOff=0.8°C` and cutoff 1.2°C. These two statements cannot be declared equivalent without clarifying whether “overshoot” means any positive target error or the configured cutoff. The 3°C qualification ceiling alone does not settle that control rule. The audit preserves both facts rather than silently changing the specification or adding a hard-zero rule.

## Hardware evidence boundary

The safely canceled default-scope run `firmware-pps5a-1791181537209` independently confirmed 60°C in two heating trials: start 34.99°C, qualified Hold 60,047ms, overshoot 0.92°C and peak-to-peak 1.67°C. Its stable window began 10,285ms after Warmup under an independently verified policy-2 extension. Sequence 0..3476 is complete; device/merged trace and five-file digests verify. All 1,232 cooldown samples have zero logical request and MOS output. The standard report is `firmware/target/hil-policy2-continuation-full-bundle/index.html` and its owner outcome remains failed/canceled.

That individual thermal result does not validate the corrected coast gates or qualify the full profile. Post-cancel status showed heating disabled, zero MOS output and cleared RAM preview. No candidate was saved. The hardware still runs `0.27.6-dev.2655786`; candidate-materialization and coast corrections need fresh committed-artifact flash/readback/HIL evidence.
