# ADC, Thermal Control, and Fan Policy

## Responsibility

Acquire calibrated VIN/RTD samples, classify sensor faults, select and
interpolate thermal-control targets, calculate PID/thermal-plant frames, and
decide fan policy.

## Inputs and Reads

- ADC1 GPIO1/GPIO2.
- Calibration/configuration, target/profile state, and latest PD observation.
- Temperature/fault state, heater state, cooling modes, and persistence or
  thermal-model locks.

## Commands and Mutations

Returns `RtdMeasurement`, heater-control snapshots, `FanPolicyDecision`, and
`FanHardwareCommand`. It does not directly write heater or fan GPIO.

## Published Outputs

- Runtime telemetry and fault reasons.
- PID/control frames.
- Fan display state and requested output commands for the runtime loop.

## Mechanism

Synchronous pure helpers plus asynchronous ADC reads called from the runtime
control deadline.

## Control Authority

Computation only. `runtime_loop` decides when to apply the result.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| ADC sample and control frame | Snapshot | Reports measured and calculated state. |
| Fan/heater command | Command | Requests output application by the owning path. |
| Sensor/temperature/model fault | Interlock | Reduces or denies heater output and may force cooling. |

## Physical Output Ownership

`apply_fan_output`, called by the runtime loop, writes GPIO35/GPIO36. Heater
requests pass through `HeaterPwmGate`; this module never performs the final
heater write.

## Safety and Failure Behavior

Sensor faults, over-temperature, cooling-disabled locks, missing thermal
models, and unavailable PD contracts reduce or deny heater output.
Over-temperature can force cooling independently of normal fan mode.

## Source References

- `firmware/src/bin/flux_purr/adc.rs`
- `firmware/src/bin/flux_purr/thermal.rs`
- `firmware/src/bin/flux_purr/fan.rs`
- `firmware/src/thermal_plant.rs`
- `firmware/src/bin/flux_purr/runtime_loop.rs`: runtime call sites
- `firmware/src/bin/flux_purr/tasks.rs`: `apply_fan_output`
