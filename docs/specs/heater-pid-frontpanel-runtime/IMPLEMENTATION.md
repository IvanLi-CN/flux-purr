# Heater PID frontpanel runtime implementation

## Current coverage

- PID heater control, fan policy, fault latching, dashboard state projection, and frontpanel runtime synchronization are implemented.
- Dashboard startup presentation uses explicit initializing, EEPROM-restore, ready, and initial-RTD-fault states so the first framebuffer cannot expose a numeric bring-up placeholder. PD readiness is represented separately as `POWER/WAIT`; the fixed post-display and pre-ADC delays are removed from the Dashboard path.
- The async display SPI device owns CS through a drop guard, so a PD-service timeout cannot leave the panel selected. USB early-control emits `boot_stage=runtime_ready` only after the first UI and startup work have completed, before the runtime loop begins.
- Host fixtures and web/native contracts cover normal operation, cooling, over-temperature, sensor faults, and safe-off behavior.
- The normalized heater/fan state is shared across firmware, devd, HTTP, and Web surfaces.
- The separate `pps5a` thermal-tuning runtime records local samples and installs candidate RAM previews at trial boundaries. Normal 5A heating and tuning select the same existing point-local profile controller when a valid physical model and verified 5A-capable source are present. Host tests show that changing a candidate approach point changes the logical request, a 3A source retains plant-PI output, and an armed source-class change disarms before the controller changes. Preview identity, basic safety disarm, and a 60-second hold trace have host coverage; final MOS behavior still needs device evidence.
- A tuning run begins with a heater-off cooldown check, device event time is relative to the run command, and missing or broken stable-window evidence rejects the trial at the full-speed-to-stable deadline. A new candidate may run only after cooldown; the triggering sample and decision remain in the event trace. Each target has a monotonic 20-minute budget including cooldown, with no separate two-trial cap; safety faults still disarm immediately.
- In the default nine-target scope, failed endpoints do not suppress the other endpoint. An interior target runs only when both recursive parent bounds were accepted; otherwise it records an interval-prune decision without heating. Eligible interior targets seed their point from the two accepted bounds, update the canonical candidate hash, and require a new RAM preview receipt at the next trial boundary.
- Trial warmup evidence requires a first sample with at least 99.5% logical request and 99% physical PWM; after that point the logical request must remain full. A physical dip may remain between 95% and 99% for at most two seconds. Lower or longer dips invalidate the trial's warmup gate, while pre-full soft-start samples do not claim full-output evidence.
- Trial decisions record the external strategy's eleven ordered score dimensions using device-local evidence. Hold PWM percentiles and the fitted approach-curve error are accumulated from the trial's own samples; missing evidence receives the explicit worst-rank sentinel rather than an invented measurement. Each target uses separate 12-second scout, current/predicted batch comparison and 60-second hold-confirm trials, with cooldown and trial-local metrics between them.

## Validation

- Firmware host tests, web unit tests, and frontpanel preview fixtures cover the runtime state model.
- Safety paths retain fail-closed behavior when sensor, control, or transport checks fail.
- Host-side frontpanel preview and state-transition tests cover placeholders, EEPROM restore, PD wait, initial sensor fault, first valid RTD promotion, and preservation of the last valid temperature after runtime faults.

## Remaining gaps

- Physical thermal-loop acceptance requires a separately authorized device run.
- Firmware tuning still uses a simple candidate mutation and has not matched the external 5A retune or evidence-specific candidate variant rules. Batch ranking uses the device-local score but lacks complete conformance fixtures against the frozen host strategy. Full 5A output, current-limit, failure and qualification behavior still requires authorized device evidence; heating HIL is not ready.
