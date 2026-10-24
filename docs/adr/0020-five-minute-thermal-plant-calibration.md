# Five-Minute Thermal Plant Calibration

## Status

Accepted

## Decision

The product `thermal_plant_auto` calibration has a five-minute wall-clock budget from its first runtime update. The budget covers ambient sampling, the protected PPS scan, fan-assisted cooling, fitting, and the final commit. A timeout disables heater output, clears the calibration-owned PPS request, retains any previously committed model, and reports `thermal_plant_timeout`; a partial trace is never activated.

The product path selects one actual FUSB302B PPS APDO that covers `20V` and at least `3A`, confirms the unloaded high-voltage contract inside the source power envelope, then requests exactly `3A` for the heated scan. It starts at the usable PPS floor, advances in `100mV` requests, and records only confirmed full-conduction observations after a `500ms` electrical observation interval. The scan stops at the first reproducible source boundary or safe source ceiling, retains the last safe 3A tracking voltage, and continues at that voltage until powered temperature coverage reaches at least `100°C`. The independent live `220°C` cutoff still disables the MOS in the same control cycle.

The rapid path preserves the whole-device boundary contract: requested voltage/current, confirmed VIN response, raw RTD and VIN ADC codes and pre-user-calibration millivolts, WiFi-driver state, divider-supply metadata, and the existing trace provenance remain persisted. It does not treat the 3A request as a live current measurement and does not infer plate resistance or auxiliary power. For the transient fit, the confirmed VIN response is paired with the fixed 3A contract ceiling and physical duty as a bounded input proxy; this keeps the model self-consistent without claiming a measured load power. Cold-start observations below `50°C` remain in the raw trace and coverage checks but are excluded from the transient fit because RTD contact and heat spreading are still settling. RTD-quantized windows with less than `1°C` observable change are also retained as raw evidence but excluded from the energy-balance rows. The existing thermal transient fit uses the powered trace; the fan-assisted cooling trace is retained for terminal validation but is excluded from the heater-input regression because its fan load is not part of that proxy. After the 100°C coverage point the heater is disabled and the cooling endpoint of `80°C` is used when time permits. The fit and commit are accepted only when the complete physical-coefficient and residual gates pass.

The previous `2.0A` through `3.0A` equilibrium-stage scanner remains available through the detailed `SupplyScan::new` constructor for host regression and historical analysis. It is not the product calibration entry point because its ten-second equilibrium windows cannot meet the five-minute owner-facing requirement. Lower-current stages therefore cannot create product calibration records or automatic-heater boundary points.

Capability gaps, negotiation failure, sensor failure, WiFi-state changes, source collapse, and timeout all fail closed. A temporary missing capability snapshot still consumes the same five-minute budget; it may pause output while waiting for a fresh snapshot, but it cannot extend the calibration indefinitely.

## Rationale

The automatic-heater protection needs a measured whole-device 3A source boundary and valid thermal transient evidence. Completing many current stages would provide additional diagnostic evidence but would make the owner-facing calibration duration unbounded relative to the required five-minute workflow. A single confirmed 3A boundary scan preserves the safety-relevant current class, the raw evidence needed for replay, and the existing physical fit while making the completion contract explicit.

## Consequences

- A 3A-capable source is still mandatory; a source below the product current floor cannot start this calibration.
- The saved boundary contains measured 3A tracking points only. It does not contain fabricated lower-current points or a resistance curve.
- A source or thermal plant that cannot reach the minimum powered coverage and complete a valid cooling/fit sequence within five minutes remains uncalibrated and keeps normal heating locked under the existing admission rules.
- The detailed scanner tests continue to protect the older equilibrium and probe invariants without changing the product runtime path.

## References

- [Heater supply boundary calibration](0019-heater-supply-boundary-calibration.md)
- [Automatic heater voltage/current boundary](0018-automatic-heater-voltage-current-boundary.md)
