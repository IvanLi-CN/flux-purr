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
