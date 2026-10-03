# Heater PID frontpanel runtime history

## Legacy identity

- Former legacy ID: `q2aw6`.

## Lifecycle

- `active`: this topic is the current heater, fan, and dashboard runtime truth source.

## Partial replacement

- It locally supersedes older heater/fan runtime wording in `frontpanel-ui-contract` and `frontpanel-input-interaction`; those topics retain their visual and input contracts.

## Firmware trial stopping boundary

The firmware engine's hard stage boundary applies to scout, both batch candidates and hold confirmation. The external runner's longer `TuningScout` capture is retained as external behavior and does not govern firmware stopping. Production controller Hold remains a profile-dependent control phase; physical stable-window and qualified-hold gates govern acceptance independently.

Commit `b9970102` enforces that boundary and prevents the report-sampling cadence from delaying a deadline or a broken pending window. Captured-trace regressions, four external-strategy candidate comparisons and authorized 60°C diagnostic HIL cover the change. The physical run verifies stopping, cooldown and complete failure evidence; its seven rejected trials do not establish a passing candidate, nine-target qualification or EEPROM persistence.

## Production startup and missing-Hold projection

The production heater soft-start requirement is 100ms for normal heating and firmware tuning alike, replacing the former one-second duration in the shared adapter. The tuning recorder preserves startup observations until full physical output so a sub-300ms ramp remains auditable. The thermal-plant calibration transient remains independent.

Hard-stop trials can finish without qualified Hold evidence. That absence must retain the external strategy's unknown/worst-rank value; projecting it as zero peak-to-peak incorrectly enabled a larger low-side acceleration. Differential fixtures from the retained `b9970102` HIL trace cover the complete next candidate and prohibit evidence after the actual stop from entering the comparison. These host checks do not establish a passing physical candidate.

The standard report displays local phase-boundary times on all chart X axes and phase durations in the temperature chart. The same template serves device reports and offline rendering; no special example-report path changes the presentation.

Commit `0aa41789` implements the shared startup duration and missing-Hold projection; `eb703ee7` implements the report time annotations. Authorized HIL on `0.27.6-dev.eb703ee` observes full output after 118–136ms, retains all intermediate startup commands and verifies the complete failure trace. The first diagnostic run improves the predicted trial's thermal metrics but fails its continuous stable-window gate, then exhausts its target budget in cooldown. The three trials do not establish a passing candidate, default qualification or persistence.

Retesting that generated candidate produces seven rejected trials and two identical current/predicted comparisons. Differential replay of every retained trial prefix matches this branch's external candidate transformation, including the lower-band correction in `dfb8423b`, while the actual thermal gate remains failed. It is not an unchanged-baseline comparison with `d86a4edd`. The current profile breaks the upper stable-band edge before the continuous window completes; the conservative prediction misses the lower band at the deadline. This evidence excludes a next-candidate transformation difference against that comparator for those traces, but does not prove complete control-path conformance or show that the hardware cannot be tuned. Separately sampled runtime status can cross an event-page cursor's phase boundary and must not be described as simultaneous evidence.

Retesting the generated lower-side candidate produces nine further rejected trials. The search changes the candidate, but its warmer prediction still leaves the upper stable-band edge before the required continuous window; conservative predictions miss the lower band. Complete, verified failure evidence and terminal heater-off cooling are retained. The fixed 3328mV RTD excitation model has not been validated by measuring this device's 3V3 rail, so these runs cannot establish the absence of temperature-conversion bias.
