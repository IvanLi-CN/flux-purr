# Automatic Heater Voltage And Current Boundary

## Status

Superseded in its calibration source and auxiliary-load accounting by [ADR 0019](0019-heater-supply-boundary-calibration.md). The contract ordering, mode boundaries, final-request cap, and pause/recovery rules remain applicable.

## Decision

Normal Auto/W65 automatic heating must limit requested supply voltage using the heater resistance-versus-temperature estimate and source current capability so that the heater's predicted conduction current stays within the available source budget. Calibration data takes priority; the preset theoretical resistance model is the fallback. This 3A boundary applies to Auto/W65 normal automatic heating, including its Fixed fallback and all entry points into that mode. Explicit W100 is a separate 5A production path: it requires an independently confirmed 5A source contract and does not use or claim protection from this 3A boundary, which has no 5A whole-device calibration record. Calibration heating and manual PPS debugging retain their existing behavior and are not covered by this automatic-heating guarantee.

The resistance estimate uses the first valid source in this order: RAM heater-curve preview, persisted raw heater observations projected with the current RTD calibration, saved active heater curve, and the preset theoretical model. Once a valid calibration curve is selected, its interpolated resistance is used directly; a separate maximum with the nominal resistance must not raise that result. Existing calibration generation, nominal anchors and floors within that generation, validation, interpolation, and endpoint rules remain unchanged. The theoretical fallback uses the existing `R20 = 3.2 ohm` and `alpha = 0.00393 / degC`.

A physically valid thermal plant model remains necessary for normal heating. When that model is valid but resistance calibration data is missing or invalid, normal heating may proceed using the theoretical resistance fallback. Missing resistance calibration must not independently lock heating or invalidate an otherwise valid thermal model solely through its heater-curve association. Other thermal-model validity, commissioning, sensor, and source-eligibility requirements remain in force.

The automatic-heating current budget reserves a fixed 200mA for board loads before assigning the remaining source current to the heater. The current capability belongs to the actual candidate PDO/APDO; fields from different advertised operating points must not be combined. An active confirmed contract may further reduce the permitted current. Current request encoding and confirmed contract limits must be reflected before heater conduction is authorized.

For voltage in volts, current in amperes, and resistance in ohms, the boundary is:

```text
I_heater_budget = max(I_source_limit - 0.200, 0)
V_limit = min(V_source_max, V_device_max, R_used(T) * I_heater_budget)
V_pps_max = floor(V_limit / 0.100) * 0.100
P_heater_max = V_allowed^2 / R_used(T)
```

`V_allowed` must be an eligible supply voltage at or below the limit. A zero remaining current budget, invalid resistance after exhausting the fallback sources, or missing usable source capability cannot authorize heater conduction. Requested PPS voltage is rounded down to the existing 100mV step so rounding cannot exceed the calculated limit. The normal controller's watt saturation and voltage mapping must use the same resistance and safe supply envelope. During PPS Warmup/full-speed, a nonzero confirmed request uses a 100% static heater gate; `warmup_soft_start_percent` must not be applied as PWM. Warmup requests the selected calibrated ceiling directly rather than applying the ordinary 500mV request ramp, so the full-conduction interval cannot oscillate between adjacent PPS contracts. After Warmup, Approach/Hold lower-power requests must keep the current confirmed PPS voltage while it remains within the latest safe ceiling and use PWM for the power reduction; a new PPS request is required only when the safe ceiling tightens or a higher safe contract is needed. Intermediate duty is reserved for explicitly low-power Approach/Hold allocation after Warmup, and is not full-speed evidence. A 100% command means all safely available heater power, not the source's nominal nameplate watts. Legacy `65w` and `100w` profile labels do not supply an independent source power budget.

Normal automatic heating may use only supply operating points whose predicted heater current during PWM conduction fits this budget. A lower advertised PPS or Fixed operating point may be used when it satisfies the existing source-eligibility rules and this current boundary. If no safe operating point exists, heating pauses with an observable reason. Reducing average PWM duty does not make an otherwise unsafe voltage eligible.

The final requested source voltage must remain within the calculated current-derived ceiling. Cable or board-path voltage-drop compensation may operate within this ceiling but may not raise it. Lower delivered heater power under voltage drop is an accepted consequence of this boundary.

When a temperature, resistance-source, or source-capability update makes the active voltage unsafe, automatic heating turns the heater output off before requesting a safe operating point. It may resume automatically within the same heating session only after a safe contract is confirmed. No safe operating point means continued pause; existing disconnect and sensor-fault shutdown rules remain authoritative. The same check applies when returning from calibration or manual PPS debugging. HOLD voltage retention, PPS soft start, request stepping, and transition delays must not keep the heater conducting at an unsafe voltage. PPS negotiation and safety blanking use a 0% gate; a confirmed full-speed PPS request resumes at static 100%, never through a 1–99% PWM ramp.

## Context And Consequences

The product distinguishes Auto/W65 target-temperature heating, the explicit W100 5A production path, calibration, and manual PPS control. Keeping the voltage limit specific to the Auto/W65 path preserves those existing calibration and debugging workflows; W100 must not be presented as protected by an unmeasured 5A version of this boundary. Future changes to W100, calibration, or debugging require their own explicit requirements rather than inheriting the 3A automatic-heating boundary.

The existing production request-ceiling helper returns the source maximum without using temperature, resistance, or the current reserve, and the separate voltage-drop compensation can raise the resulting request. The agreed boundary therefore changes automatic control behavior rather than merely adding a diagnostic estimate. With `R_used = 3.2 ohm` and a `20V / 3.25A` source, the heater budget is `3.05A`, the current-derived limit is `9.76V`, the PPS ceiling is `9.7V`, and the modeled maximum heater power is approximately `29.4W`. Cold-start heating can be slower; it must not recover nominal source watts by exceeding the voltage boundary.

The calibration-derived resistance is an estimate. Its observations currently combine measured heater voltage with negotiated contract current, not an independently measured instantaneous load current. The safety claim is therefore a bound on the modeled load using the selected resistance data. This decision does not add current sensing or change calibration acquisition. Actual source-load behavior requires physical validation under a separately authorized device run.

## References

- [Domain language](../../GLOSSARY.md)
- [Heater PID frontpanel runtime](../specs/heater-pid-frontpanel-runtime/SPEC.md)
- [Heater plate electrical model](../hardware/heater-plate-design.md)
- [Resistance selection and production PPS request ceiling](../../firmware/src/bin/flux_purr/eeprom.rs)
- [Automatic output and transition handling](../../firmware/src/bin/flux_purr/tasks.rs)
