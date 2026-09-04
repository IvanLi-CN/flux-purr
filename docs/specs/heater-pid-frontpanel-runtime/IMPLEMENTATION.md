# Heater PID frontpanel runtime implementation

## Current coverage

- PID heater control, fan policy, fault latching, dashboard state projection, and frontpanel runtime synchronization are implemented.
- Host fixtures and web/native contracts cover normal operation, cooling, over-temperature, sensor faults, and safe-off behavior.
- The normalized heater/fan state is shared across firmware, devd, HTTP, and Web surfaces.

## Validation

- Firmware host tests, web unit tests, and frontpanel preview fixtures cover the runtime state model.
- Safety paths retain fail-closed behavior when sensor, control, or transport checks fail.

## Remaining gaps

- Physical thermal-loop acceptance requires a separately authorized device run.
