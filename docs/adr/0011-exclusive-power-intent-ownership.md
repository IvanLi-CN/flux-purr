# Exclusive Power Intent Ownership

## Status

Accepted

## Context

Flux Purr has normal thermal control, operator manual PPS, manual calibration, and `thermal_plant_auto` as distinct sources of contract intent. Their existing safety expectations conflict with last-writer-wins behavior: in particular, `thermal_plant_auto` must not accept a manual PPS override.

## Decision

`PowerCoordinator` admits requests from exactly one `PowerIntentOwner` at a time, with priority `ThermalPlantAuto > Calibration > ManualOperator > AutomaticThermal > Idle`. A higher-priority transition atomically supersedes every lower-priority accepted or in-flight request, which completes with `Superseded`; leaving an owner requires an explicit next request or `Idle`, never revival of a stale target. PD/source failure is not an owner and unconditionally revokes the heater permit and active contract.

## Consequences

Every contract transition has an accountable product-policy source and a terminal outcome. Product modes must explicitly acquire and release their intent rather than rely on timing between independently queued writes. The coordinator needs bounded per-owner admission and completion state, while PD service safety remains independent of ownership or notification delivery.
