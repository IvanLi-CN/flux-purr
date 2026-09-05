# Heater PID frontpanel runtime implementation

## Current coverage

- PID heater control, fan policy, fault latching, dashboard state projection, and frontpanel runtime synchronization are implemented.
- Dashboard startup presentation uses explicit initializing, EEPROM-restore, ready, and initial-RTD-fault states so the first framebuffer cannot expose a numeric bring-up placeholder. PD readiness is represented separately as `POWER/WAIT`; the fixed post-display and pre-ADC delays are removed from the Dashboard path.
- Host fixtures and web/native contracts cover normal operation, cooling, over-temperature, sensor faults, and safe-off behavior.
- The normalized heater/fan state is shared across firmware, devd, HTTP, and Web surfaces.

## Validation

- Firmware host tests, web unit tests, and frontpanel preview fixtures cover the runtime state model.
- Safety paths retain fail-closed behavior when sensor, control, or transport checks fail.
- Host-side frontpanel preview and state-transition tests cover placeholders, EEPROM restore, PD wait, initial sensor fault, first valid RTD promotion, and preservation of the last valid temperature after runtime faults.

## Remaining gaps

- Physical thermal-loop acceptance requires a separately authorized device run.
