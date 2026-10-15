# Heater PID frontpanel runtime implementation

## Current coverage

- PID heater control, fan policy, fault latching, dashboard state projection, and frontpanel runtime synchronization are implemented.
- Dashboard startup presentation uses explicit initializing, EEPROM-restore, ready, and initial-RTD-fault states so the first framebuffer cannot expose a numeric bring-up placeholder. PD readiness is represented separately as `POWER/WAIT`; the fixed post-display and pre-ADC delays are removed from the Dashboard path.
- The async display SPI device owns CS through a drop guard, so a PD-service timeout cannot leave the panel selected. USB early-control emits `boot_stage=runtime_ready` only after the first UI and startup work have completed, before the runtime loop begins.
- Host fixtures and web/native contracts cover normal operation, cooling, over-temperature, sensor faults, and safe-off behavior.
- The normalized heater/fan state is shared across firmware, devd, HTTP, and Web surfaces.
- Normal automatic heating now uses a dedicated `automatic_heater` policy. It resolves resistance from RAM preview, projected raw observations, saved curve, or the theoretical `3.2 ohm` model; floors each selected APDO current to the protocol step, reserves `200mA`, derives a per-APDO voltage ceiling and `Pmax`, and selects PPS before a safe Fixed fallback. Calibration heating and manual PPS retain their existing electrical paths.
- The runtime carries a watt request from the thermal controller into the output task. Automatic PPS voltage is derived from `sqrt(Ptarget * R)` and the confirmed contract computes PWM power; every nonzero write rechecks the current resistance, source contract, and safety ceiling. Unsafe contracts clear PWM before a direct safe request, and a confirmed stable replacement can resume the same session.
- Automatic PPS recovery treats an absent settled observation as unsafe once no PD operation is pending, so a same-voltage contract is re-requested after a failed or interrupted negotiation. No-safe-candidate pauses are surfaced as `no-safe-heater-power` in runtime status and the front-panel warning state.
- Thermal-model admission no longer requires a persisted resistance curve or matching curve transaction. A valid thermal plant, commissioning state, sensor state, and source capability remain required; the automatic heater uses the theoretical resistance when calibration data is absent.

## Validation

- Firmware host tests, web unit tests, and frontpanel preview fixtures cover the runtime state model.
- Safety paths retain fail-closed behavior when sensor, control, or transport checks fail.
- Host-side frontpanel preview and state-transition tests cover placeholders, EEPROM restore, PD wait, initial sensor fault, first valid RTD promotion, and preservation of the last valid temperature after runtime faults.
- Firmware host tests cover current encoding, resistance-source priority, 200mA budget calculations, per-APDO selection, confirmed-contract power ceilings, watt-to-voltage/PWM mapping, fixed-voltage rejection, path-drop ceiling enforcement, theoretical admission, actuator-paused integral freeze, non-blocking PD ticket terminal matching, legacy calibration-path behavior, and the host fake-PD/PWM transition decisions used by the asynchronous output path.

## Remaining gaps

- Physical thermal-loop acceptance requires a separately authorized device run.
- External current measurement and the authorized `3A` HIL run remain required to prove that the negotiated source stays online and that PWM conduction current follows the model. A separate `5A` run is outside the accepted validation scope.
