# Status-Light State and Task

## Responsibility

Select a deterministic state-priority language and translate it into full RGB
on/off patterns for the common-anode status LED.

## Inputs and Reads

- Boot elapsed time.
- Thermal runaway/attention, sensor fault, cooling-disabled lock, heater
  interlock, calibration, heater-enabled, and fan-enabled state.

## Commands and Mutations

Runtime and boot paths call `set_status_light_state`. The task samples the
state and updates RGB channels every `20 ms`.

## Published Outputs

- Selected state in `STATUS_LIGHT_STATE`.
- Final RGB channel output.

## Mechanism

One atomic state code from runtime/boot paths to the dedicated status-light
task. There is no HTTP or mailbox path.

## Control Authority

The runtime loop selects semantic state. `run_status_light_task` owns final
GPIO writes.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| Semantic status state | Command/selection | Requests a visual state. |
| `STATUS_LIGHT_STATE` | Snapshot | Publishes the current selected state. |
| State priority | Interlock | Higher-severity state suppresses ordinary indication. |

## Physical Output Ownership

`run_status_light_task` and `apply_status_light_output` own GPIO39/38/37. The
channels are active-low for the common-anode LED.

## Safety and Failure Behavior

`ThermalRunaway`, pending attention, sensor fault, cooling-disabled
overtemperature, and heater interlock preempt ordinary states in that order.
Pure green remains reserved for ROM download mode.

## Source References

- `firmware/src/status_light.rs`: `StatusLightInputs`,
  `select_status_light_state`, `status_light_output`
- `firmware/src/bin/flux_purr/tasks.rs`: `STATUS_LIGHT_STATE`,
  `run_status_light_task`, `apply_status_light_output`
