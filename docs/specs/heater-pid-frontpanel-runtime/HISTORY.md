# Heater PID frontpanel runtime history

## Legacy identity

- Former legacy ID: `q2aw6`.

## Lifecycle

- `active`: this topic is the current heater, fan, and dashboard runtime truth source.

## Partial replacement

- It locally supersedes older heater/fan runtime wording in `frontpanel-ui-contract` and `frontpanel-input-interaction`; those topics retain their visual and input contracts.

## Braking-first model Approach

The owner fixes the normal tuning minimum at 5% and requires an immediate minimum-power request at Warmup-to-Approach entry. The former loss-plus-nonnegative-rise law cannot reduce output below the complete target-loss estimate and does not implement this braking trajectory. Policy 4 binds the 50-permille law and preserves original deadlines, conditional-extension prerequisites, physical Hold entry and independent confirmation. Policy 3 remains the historical 100-permille model policy; old evidence is not rewritten. The 50% reserve relative to stable Hold average is an observed margin, not guaranteed by the fixed 5% floor at 60°C.

Artifact 7cf64a98 completes a verified nine-target run after the braking-first correction. Every target passes an independent 60-second confirmation within its original stable-window deadline, without conditional extension. All 36 initial Approach requests are 5%; no normal Approach sample is zero, and all physical Hold entrances meet the fixed gate. The 60°C stable Hold mean remains only 5.61516%, so the additional half-mean reserve is not established. Thermal gate evidence and overall qualification remain separate: the retained candidate has a matching RAM receipt but no EEPROM save/readback receipt. See [braking-first HIL](HIL-BRAKING-FIRST-APPROACH.md).

## Historical firmware trial stopping boundary

The owner authorizes warmer firmware-only high-temperature starts after reviewing recorded Warmup segments. The >160°C boundary retains160°C at its previous120°C start, while180/220/240°C use150/200/220°C respectively. The >200°C tier takes precedence; exactly200°C stays in the target−30°C tier. Trial metadata preserves historical thresholds and actual phase durations. This replaces the uniform target−40°C rule only above160°C; the frozen external runner and all physical acceptance gates remain unchanged.

The extension contract fixes qualification at a real 100‰ logical floor. Candidate Hold baselines cannot raise that boundary. Policy `2` replaces policy `1`'s universal Hold-derived eligibility and retrospective activation after entering Hold; the first original-deadline check binds the decision. Historical records retain their actual policy and device decisions. This correction does not remove protection, change production Hold parameters, reset budgets or infer a hardware limit from one candidate.

The firmware engine's ordinary hard stage boundary applies to scout, both batch candidates and hold confirmation. The [approved conditional policy](FIRMWARE-TUNING-REMEDIATION-CONTRACT.md) is a bounded exception for an observed allowed-floor candidate after its original deadline expires; it retains all other physical qualification and safety gates. The external runner's longer `TuningScout` capture is retained as external behavior and does not govern firmware stopping. Production controller Hold remains a profile-dependent control phase; physical stable-window and qualified-hold gates govern acceptance independently.

Commit `b9970102` enforces that boundary and prevents the report-sampling cadence from delaying a deadline or a broken pending window. Captured-trace regressions, four external-strategy candidate comparisons and authorized 60°C diagnostic HIL cover the change. The physical run verifies stopping, cooldown and complete failure evidence; its seven rejected trials do not establish a passing candidate, nine-target qualification or EEPROM persistence.

## Production startup and missing-Hold projection

Conditional timing distinguishes an observed floor from a protective zero request. The zero request cannot prove a floor, but clearing a prior real witness on zero imposed an extra requirement beyond the documented rule that later higher requests invalidate it. Default-scope 80°C confirmation trial 5 in `firmware-pps5a-1791132999690` exposed that loss exactly at its ordinary deadline. The correction retains the witness for an unchanged candidate through zero while preserving invalidation by other nonzero requests, fault disarm, finite deadlines and all thermal qualification gates. It does not revise historical decisions or claim that the rejected confirmation would have passed.

Firmware tuning's approved high-temperature exception extends the existing conservative high-side candidate transform to 160–240°C and gives it precedence on failed confirmation as well as trial tuning. Earlier 160°C traces combined genuine upper-band failure with an Underpowered curve label, but the conservative branch excluded targets above 150°C, allowing repeated request increases. The exception retains measured-high-side eligibility and all acceptance gates; it does not change the frozen external runner or production PID equations. It is an intentional strategy difference and is verified separately from baseline conformance.

Captured production-runtime replay of 160°C scout3 in run `firmware-pps5a-1791220770113` exposes a second restriction: an unverified high-side stability failure with overshoot1.60°C/p2p1.67°C bypasses the conservative branch because neither metric exceeds3°C. The next floor rises700→760‰ despite the measured161.71°C upper-band exit. High-temperature stable-window failure therefore triggers the approved bounded trim independently of the thermal hard limits. A completed verified window's later ordinary exit does not trigger that exception. Low-side, absent evidence, safety stops and the frozen external/≤150°C strategy retain their boundaries.

The physical Approach→Hold entry is decoupled from the candidate's curve parameter. The observed 160°C trial entered controller Hold at 157.49°C because `holdEntryCentiC=270` also widened phase admission. Production control and tuning observation now share the existing target −0.5°C / ±3°C physical gate. Curve shaping, coast protection and blending retain their parameter semantics; no new slope or stored-energy gate is introduced. This supersedes the profile-dependent phase-entry interpretation above. Historical device records retain their actual early transitions and cannot prove the corrected behavior.

The production heater soft-start requirement is 100ms for normal heating and firmware tuning alike, replacing the former one-second duration in the shared adapter. The tuning recorder preserves startup observations until full physical output so a sub-300ms ramp remains auditable. The thermal-plant calibration transient remains independent.

Hard-stop trials can finish without qualified Hold evidence. That absence must retain the external strategy's unknown/worst-rank value; projecting it as zero peak-to-peak incorrectly enabled a larger low-side acceleration. Differential fixtures from the retained `b9970102` HIL trace cover the complete next candidate and prohibit evidence after the actual stop from entering the comparison. These host checks do not establish a passing physical candidate.

The standard report displays local phase-boundary times on all chart X axes and phase durations in the temperature chart. The same template serves device reports and offline rendering; no special example-report path changes the presentation.

Commit `0aa41789` implements the shared startup duration and missing-Hold projection; `eb703ee7` implements the report time annotations. Authorized HIL on `0.27.6-dev.eb703ee` observes full output after 118–136ms, retains all intermediate startup commands and verifies the complete failure trace. The first diagnostic run improves the predicted trial's thermal metrics but fails its continuous stable-window gate, then exhausts its target budget in cooldown. The three trials do not establish a passing candidate, default qualification or persistence.

Retesting that generated candidate produces seven rejected trials and two identical current/predicted comparisons. Differential replay of every retained trial prefix matches this branch's external candidate transformation, including the lower-band correction in `dfb8423b`, while the actual thermal gate remains failed. It is not an unchanged-baseline comparison with `d86a4edd`. The current profile breaks the upper stable-band edge before the continuous window completes; the conservative prediction misses the lower band at the deadline. This evidence excludes a next-candidate transformation difference against that comparator for those traces, but does not prove complete control-path conformance or show that the hardware cannot be tuned. Separately sampled runtime status can cross an event-page cursor's phase boundary and must not be described as simultaneous evidence.

Retesting the generated lower-side candidate produces nine further rejected trials. The search changes the candidate, but its warmer prediction still leaves the upper stable-band edge before the required continuous window; conservative predictions miss the lower band. Complete, verified failure evidence and terminal heater-off cooling are retained. The fixed 3328mV RTD excitation model has not been validated by measuring this device's 3V3 rail, so these runs cannot establish the absence of temperature-conversion bias.

## Production coast contract conformance

Two deterministic controller regressions establish inherited deviations between the production compatibility anchor and the literal coast guards: the actual Approach coast bound included an extra Hold-entry/temperature allowance, and coast release omitted filtered low-temperature error. The owner approved removing that allowance and requiring filtered low error. The corrected production path retains raw cooling, negative filtered slope, two consecutive observations and phase/blend resets. Candidate search, physical Hold admission, timing qualification, PD service and actuator safety equations are unchanged by this correction.

The static-brake/slope-times-lead rule originated in the 2026-07-11 dense thermal HIL contract (`9ba38aea`). The normal/tuning 5A profile-controller selection originated in the 2026-09-26 controller-boundary contract (`505d4c98` and ADR 0015). The absence of calibrated natural-cooling input from 5A braking is a source-audit result, not a normative prohibition.

The 5c25a71 nine-target run exposed an interrupted-export identity defect: after an application restart, the retained journal's final candidate hash differed from its sole complete startup profile. Report projection previously combined those two facts. A failing renderer regression reproduces this mismatch; projection now marks current parameters unavailable instead of substituting the historical profile. The subsequent 220°C diagnostic actually started with identical startup-profile fields, so the earlier suspected import rounding is not established by those records.

A separate import regression shows that the legacy candidate model drops global wire settings and reconstructs them from the first point. V2 seed import now bypasses that projection, preserving independent settings and point values, and verifies any declared candidate hash before obtaining a device lease. This corrects profile identity and prevents reuse of the stale interrupted export; it does not change the controller, search strategy, PD service or thermal qualification gates.

Commit `0dfafa26` implements the owner-approved coast guards. Authorized no-backup flash and exact-artifact HIL confirm the candidate-materialization readbacks and independently confirm 60°C. The default scope fails at 240°C after 15 trials using two distinct complete points; trace and five-file digests verify, with no saved candidate and heater-off cleanup. A retained 0.05°C adjacent-observation drop threshold still imposes a control-frequency-dependent release condition. A host diagnostic demonstrates coast retention during smooth 0.5°C/s cooling at 20Hz; this is a separate production-control question, not permission to alter the existing gate or claim that the hardware cannot qualify. See [coast guard HIL](HIL-COAST-GUARDS.md).
