# Natural-cooling control and fan evidence

## Artifact and test boundary

The authorized device is `288485681c38`, reached only through the owner's exact MCU port and the repository CLI/devd. The bench source is IsolaPurr `f293cc9c139e`, USB-C `port_c`. No PD-module source changes are included in the natural-cooling work. EEPROM backup was explicitly bypassed; no archive was created and EEPROM health is unknown.

Firmware `0.27.6-dev.89851eb`, source `89851eba82a586bcc14fbf304694c1fe061e4bec`, was written and verified. ELF SHA-256 is `7fdc5aadeace227e9bc14e2d291627f8ca34879fc4037e435e4ee7bb113e1a85`. After a controlled USB-C off/on cycle, device identity matched, PPS was 12V/5A, the saved model was active, HOT was LOW, and the heater and RAM preview were off.

The active model transaction was `1348419659`, with convection `139.1813mW/°C`, radiation `4.3978307e-7mW/K⁴`, capacity `13996.655mJ/°C` and delay `1950ms`. These are fitted parameters, not measured electrical power or a fan-assisted cooling curve.

## Input-chain diagnostic

Run `firmware-pps5a-1791195189293` used the standard candidate export as `--seed-profile-file`. The host unpacker stopped at `profile` instead of `profile.value`, then silently generated default points. The device trial facts exposed the mismatch: the intended 60°C floor was `100‰`, while the actual initial point had `510‰`. The run was canceled, retained a complete terminal trace and failed report, and did not save a candidate. Post-run readback confirmed heater off and preview cleared. This run cannot establish the requested seed's performance.

Commit `5dab7498` repairs v2 export import, retains explicit point-local zero timing values and rejects missing settings, missing targets and wrong source class before acquiring a device lease. Regression tests verify complete parameters and canonical hash through export/import/materialization; old plain-profile input remains supported.

## Complete run with the intended seed

Run `firmware-pps5a-1791195991823` used the corrected host and the same flashed artifact, with the complete default nine-target scope. The actual device's initial 60°C point matches the supplied retained candidate: brake `1480centi°C`, Approach floor/ceiling `100/270‰`, damping `4000‰`, Hold/reheat `100/100‰`.

| Target/trial | Result | Overshoot | Qualified-Hold p2p | Stable-window start after Warmup | Confirmation |
| --- | --- | --- | --- | --- | --- |
| 60°C scout 0 | Passed | 1.09°C | 1.55°C | 9913ms / original 10000ms | Short scout |
| 60°C confirm 1 | Adopted | 1.05°C | 1.64°C | 11451ms / conditionally bound 30000ms | Independent 60s |
| 240°C scout 0 | Rejected | 0°C recorded before stop | Unavailable | No window / original 5000ms | None |

The 240°C point was already saturated at `1000‰` Approach/Hold/reheat. Model-assisted braking ended Warmup at about `231.71°C`; the trial then missed the 5-second stable-window-start deadline. The strategy's next sanitized complete point equaled the current point, so the runtime emitted `candidate_search_exhausted_no_distinct_safe_proposal` instead of heating the same proposal repeatedly. The seven interior targets were pruned because the 240°C bound was not accepted. This demonstrates the stagnation stop, not a qualified 240°C controller or proof of hardware limits.

The five-file report is under `firmware/target/hil-natural-cooling-full-bundle/`; its raw journal is under `firmware/target/hil-natural-cooling-full-runs/firmware-pps5a-1791195991823/`. All 1583 events over `0..1582` are contiguous. Rust device verification and independent Web recomputation match digest `e27750e893573471bf8f2eaf40b24c3f9b1d5e9b71b1da1468d86ee2a2409bb7`; all five file digests match their documented bases. The full owner outcome is `failed`, review is complete and no candidate was saved.

All 1555 sample events contain the applied fan object. Of 657 cooldown samples, 656 show enable on/PWM `0‰` and zero logical/final heater output. The first initial-cooldown sample at 30ms instead retains the preceding idle fan command. This is a real boundary-ordering defect; the report preserves it rather than replacing it with a predicted command.

Browser functional checks on the standard generated report verify the fifth fan step chart, changing sample ranges with trial selection, all five linked crosshairs at the same sequence, fan enable/PWM/level in the tooltip, and a 393px mobile layout without page overflow. No screenshot or physical fan-speed measurement is claimed.

## Corrected firmware and remaining hardware proof

Commit `5a15fff1` corrects the forecast: observed net rise already includes passive loss, so inferred delayed heat must also subtract passive loss during the delay. A production-path regression with the captured model and a slowly rising high-temperature state fails before this correction by entering Approach too early, then passes after it. The shared cooling policy is also applied before the first recorded cooldown sample.

The corrected release artifact is `0.27.6-dev.5a15fff`, ELF SHA-256 `76a94fa4b58ff19c0ef585011257b7bd89d596ac0a1e78839a9e3bd2c1744d3b`. Its 87 focused heater tests, three cooldown tests, release build and boot-stack check pass; CPU0 stack headroom is `96036B`, nested PD headroom `94468B`. The no-reset flash attempt could not connect to the running application. The standard automatic-reset attempt lost the authorized port during connection, before write/verification; MCU operations stopped as required. Corrected-artifact HIL remains pending the owner's restoration of download mode on the same port.

Full qualification, successful 240°C recovery, all seven interior targets and any explicit save receipt remain unproven. The passed 60°C target on the earlier artifact does not qualify the corrected artifact or the complete batch.
