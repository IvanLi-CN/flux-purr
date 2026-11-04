# Passive Thermal Plant Cooling

## Status

Accepted

## Context

The thermal-plant calibration is intended to identify the heater plate's
unassisted thermal loss and the heat that continues to arrive after the heater
MOS is cut off. The previous rapid path enabled the fan at its full profile and
raised the calibration-owned PPS request to the fan working floor. That output
is useful for safety cooling, but it adds an uncontrolled thermal path to the
trace and does not measure the plant's natural cooling curve.

## Decision

After the protected scan reaches its heating cutoff or coverage endpoint,
`thermal_plant_auto` keeps the heater disabled, leaves the calibration-owned
PPS contract unchanged, and forces the normal fan decision to `Disabled` for
the cooling phase. It does not mutate the user's post-heat or fan-guard
preferences and it does not request a fan working contract.

General safety protections remain authoritative. If a safety path actually
enables the fan during cooling, the calibration fails closed before recording a
contaminated sample or committing a model. A live passive cooling sample uses
marker `255`. Marker `254` remains decodable for safety-intervened or legacy
fan-assisted evidence and is not eligible for `HeatResponse` fitting.

The five-minute wall-clock budget and existing failure behavior remain in
force. A natural cooling run that cannot reach the endpoint and pass the
existing fit gates within the budget fails without replacing the last valid
model.

## Consequences

- The saved cooling tail represents the heater plate and enclosure without an
  intentional fan load.
- The calibration-owned PPS request may remain above the idle contract while
  the heater is off, but it is never raised solely to power the fan.
- Existing fan-assisted traces remain decodable for storage compatibility, but
  they cannot produce a natural post-cutoff heat response. A new calibration
  run is required for that response.
- Safety cooling behavior outside the calibration contract is unchanged.

## References

- [Five-minute thermal plant calibration](0020-five-minute-thermal-plant-calibration.md)
- [Heater supply boundary calibration](0019-heater-supply-boundary-calibration.md)
- [Natural cooling control](../specs/heater-pid-frontpanel-runtime/NATURAL-COOLING-CONTROL.md)
