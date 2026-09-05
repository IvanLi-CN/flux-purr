# Heater PID frontpanel runtime implementation

## Current coverage

- PID heater control, fan policy, fault latching, dashboard state projection, and frontpanel runtime synchronization are implemented.
- Dashboard startup presentation uses an explicit initializing/ready/initial-RTD-fault state so the first framebuffer cannot expose a numeric bring-up placeholder; the fixed post-display delay is replaced by cooperative early-control service points.
- Host fixtures and web/native contracts cover normal operation, cooling, over-temperature, sensor faults, and safe-off behavior.
- The normalized heater/fan state is shared across firmware, devd, HTTP, and Web surfaces.

## Validation

- Firmware host tests, web unit tests, and frontpanel preview fixtures cover the runtime state model.
- Safety paths retain fail-closed behavior when sensor, control, or transport checks fail.
- Host-side frontpanel preview and state-transition tests cover placeholder, initial sensor fault, first valid RTD promotion, and preservation of the last valid temperature after runtime faults.

## Remaining gaps

- Physical thermal-loop acceptance requires a separately authorized device run.
