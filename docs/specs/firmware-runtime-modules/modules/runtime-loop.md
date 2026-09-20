# Runtime Loop

## Responsibility

Serialize product control, combine sensor/PD/persistence facts, run the heater
control cycle, project UI state, and decide when to redraw the front panel.

## Inputs and Reads

- Front-panel input and USB JSONL.
- The bounded `CONTROL_MAILBOX` and response state.
- PD snapshots, VIN/RTD samples, memory/configuration, and calibration state.
- Network summary, fault latches, persistence state, and task-owned signals.

## Commands and Mutations

- Invoke the control-plane adapter for USB and LAN product mutations.
- Revalidate queued LAN lease and revision preconditions.
- Request PD voltage changes, compute heater/fan outputs, persist EEPROM
  domains, and enter recovery on transport/display faults.

## Published Outputs

- LAN responses through `net::respond_to_command`.
- USB JSONL responses and runtime/UI snapshots.
- `STATUS_LIGHT_STATE`, buzzer requests, fan/heater command state, and
  persistence fault state.

## Mechanism and Ordering

`run_runtime_loop` is the serialized executor. Its normal order is:

1. Record the runtime heartbeat and apply the latest PD snapshot.
2. Pump USB responses, drain one bounded USB JSONL line, then consume at most
   one LAN mailbox command when USB did not claim the iteration.
3. Sample front-panel input and run the control adapter.
4. On the control deadline, read VIN/RTD, update fault/calibration state,
   advance heater control, and submit requested heater/PD work.
5. Commit deferred EEPROM domains, reconcile persistence and cooling safety,
   update fan output, import the network snapshot, and select status/buzzer
   state.
6. Flush the display when redraw is pending.

## Control Authority

This is the sole consumer of `CONTROL_MAILBOX` and the only executor for
LAN/USB product mutations. It computes normal heater duty and calls the fan
helper, but it is not the unconditional heater owner: the `HeaterPwmGate`, PD
permit, and watchdog can revoke output asynchronously.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| USB/LAN control line | Command | Executed in the serialized loop after validation. |
| PD and network state | Snapshot | Read and folded into the next control decision. |
| Persistence, PD, display, and permit failures | Interlock | Safety reconciliation can deny or revoke heat. |

## Physical Output Ownership

- Normal fan output is written by `apply_fan_output` when called by the loop.
- Heater duty is finally written through `HeaterPwmGate`.
- Buzzer and RGB writes belong to their dedicated tasks.
- Display SPI transfer is performed by the display I/O path.

## Safety and Failure Behavior

- Reject expired LAN leases and stale revisions before mutation.
- Force heater off on persistence, PD, or display faults while retaining
  cooling policy.
- A display failure clears adjustable-PD ownership, drives full cooling, and
  enters the USB-readable recovery path.

## Source References

- `firmware/src/bin/flux_purr/runtime_loop.rs`: `run_runtime_loop`
- `firmware/src/bin/flux_purr/runtime_loop.rs`: `runtime_process_usb_input`,
  `runtime_process_lan`, `runtime_control_heater`,
  `runtime_persist_and_update_safety`, `runtime_refresh_display`
- `firmware/src/net.rs`: `CONTROL_MAILBOX`, `respond_to_command`
