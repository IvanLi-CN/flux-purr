# Source-class thermal controller selection

## Status

Accepted

## Context

The external 5A tuning strategy changes point-local thermal profile parameters. The physical-plant predictive PI controller used for ordinary heating does not read those parameters. A RAM preview can therefore pass readback while having no effect on the heater. The 3A plant controller and protected thermal-plant calibration have separate, established safety behavior.

## Decision

- With a valid physical thermal model and a verified 5A-capable PPS source, ordinary 5A heating and firmware-owned 5A tuning use the existing point-local `HeaterController::update_at` path. Preview, saved and default profiles resolve through the same effective-parameter selection. Tuning cannot choose a separate heater controller.
- A 3A source retains the existing physical-plant predictive PI path. An explicit profile-mode selection alone never upgrades a 3A source to the 5A path.
- The physical thermal model remains an ordinary-heating admission requirement for both source classes. The existing source capability, current, temperature, sensor, overtemperature, EEPROM, PD-contract and actuator checks remain authoritative. `Pmax(T)` is a modeled electrical ceiling, not measured physical power; the production PPS/PWM chain enforces the source and heater-current boundaries without reporting measured watts.
- Controller selection changes only at a disarmed boundary. A source-class change while heating disarms and resets control state before another arm. The independent `thermal_plant_auto` transient remains unchanged.
- Exact output parity with the former 5A plant PI is not a compatibility requirement. Required parity is 3A plant behavior, 5A normal/tuning path identity, effective profile semantics, and all shared safety boundaries.

## Consequences

Existing external 5A tuning parameters can affect the actual production heater path without copying its host runner or inventing a tuning-only output clamp. Ordinary 5A behavior differs from the former plant PI, so software parity tests and authorized full-scope HIL evidence must validate warmup, overshoot, settling, hold stability and fail-safe behavior before delivery.

## Alternatives considered

- Keeping the plant PI for 5A and translating its fixed gains into profile parameters would redefine the external 5A strategy and invalidate the frozen candidate vectors.
- Running a separate profile controller only during tuning would make passing candidates non-representative of ordinary heating and duplicate the safety-critical control path.
