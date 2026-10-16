# Heater PID frontpanel runtime implementation

## Current coverage

- PID heater control, fan policy, fault latching, dashboard state projection, and frontpanel runtime synchronization are implemented.
- Dashboard startup presentation uses explicit initializing, EEPROM-restore, ready, and initial-RTD-fault states so the first framebuffer cannot expose a numeric bring-up placeholder. PD readiness is represented separately as `POWER/WAIT`; the fixed post-display and pre-ADC delays are removed from the Dashboard path.
- The async display SPI device owns CS through a drop guard, so a PD-service timeout cannot leave the panel selected. USB early-control emits `boot_stage=runtime_ready` only after the first UI and startup work have completed, before the runtime loop begins.
- Host fixtures and web/native contracts cover normal operation, cooling, over-temperature, sensor faults, and safe-off behavior.
- The normalized heater/fan state is shared across firmware, devd, HTTP, and Web surfaces.
- Calibration and automatic requests pin the selected APDO through refresh, ticket completion and every conduction check. The original advertised voltage range remains separate from the usable 5.5V scan floor and 21V ceiling. Observed-temperature windows stop at 1°C above each projected point; a complete calibration/reheat fixture checks actual boundary use and reaches the cutoff.
- The automatic policy selects a safe actual APDO using whole-device boundary observations and the current raw-RTD projection, WiFi driver state and advertised source envelope. Uncovered regions use the labelled preset electrical model. Legacy resistance curves and derived thermal models are absent; no fixed auxiliary-current reserve is subtracted. The watt reference remains explicitly estimated from the preset model.
- The runtime carries a watt request from the thermal controller into the output task. Automatic PPS voltage is derived from `sqrt(Ptarget * R)` and the confirmed contract computes PWM power; every nonzero write rechecks the current boundary, latest confirmed PD service snapshot, source contract, and safety ceiling. Unsafe contracts clear PWM before a direct safe request, and a confirmed stable replacement can resume the same session through a fresh `1000ms` automatic soft-start, including when the thermal phase remains `Approach` or `Hold`.
- Automatic fallback drains or waits for an existing PPS ticket before requesting a Fixed contract, so a single pending-ticket slot cannot be overwritten while an older negotiation is still unresolved.
- Automatic PPS recovery treats an absent settled observation as unsafe once no PD operation is pending, so a same-voltage contract is re-requested after a failed or interrupted negotiation. No-safe-candidate pauses are surfaced as `no-safe-heater-power` in runtime status and the front-panel warning state.
- Thermal-model admission no longer requires a persisted resistance curve or matching curve transaction. A valid thermal plant acquired with the boundary format, commissioning state, sensor state and source capability remain required. A missing applicable boundary region uses the preset fallback; an obsolete thermal calibration cannot satisfy admission.

## Validation

- Firmware host tests, web unit tests, and frontpanel preview fixtures cover the runtime state model.
- Safety paths retain fail-closed behavior when sensor, control, or transport checks fail.
- Host-side frontpanel preview and state-transition tests cover placeholders, EEPROM restore, PD wait, initial sensor fault, first valid RTD promotion, and preservation of the last valid temperature after runtime faults.
- Targeted host tests cover quantized full-power endpoints, exact low-current calibration RDO encoding, raw boundary replay, 3A transient coverage with a simulated CC source, same-cycle cutoff/WiFi disarm, and the maximum 765-byte combined EEPROM record. VM host verification passes 366 library and 404 runtime tests, the related PD suites, strict host/Xtensa Clippy and the Rust structure checker. Web boundary-contract tests and devd mock/transport tests pass. Physical acceptance remains pending.

- Product calibration retains up to 21 equilibria and 20 transient 3A points, then fits and persists the same compacted trace. Samples require confirmed contracts and 100% conduction. Negotiation pauses are recorded as zero output; real elapsed time drives equilibrium windows.

## Remaining gaps

- Physical thermal-loop acceptance remains pending on the owner-authorized 7cm plate and 60W/3A supply chain.
- External current measurement and the authorized `3A` HIL run remain required to prove that the negotiated source stays online and that PWM conduction current follows the model. A separate `5A` run is outside the accepted validation scope.
