# Control-Plane Adapter

## Responsibility

Parse and validate USB JSONL-equivalent commands, map runtime state to bounded
wire payloads, and execute the domain operation supplied by the runtime owner.
It is an adapter, not an independent executor.

## Inputs and Reads

- A control line and request ID.
- Runtime-owned references to memory, calibration, PD, EEPROM, heater, fan,
  buzzer, and telemetry state.

## Commands and Mutations

Handles runtime configuration, calibration, thermal-plant, heater-curve,
network, persistence, and status operations through supplied references.
LAN commands use the same normalized line format through
`lan_command_to_control_line`.

## Published Outputs

- `UsbFrame` responses.
- Status projections consumed by the USB or LAN adapter.

## Mechanism

Newline-delimited JSONL parsing and typed domain calls. The runtime loop owns
the invocation boundary and response transport.

## Control Authority

The adapter validates and dispatches only. It does not schedule a second
product executor or own any physical output.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| JSONL request | Command | Validated and executed under runtime-loop ownership. |
| Status projection | Snapshot | Encodes runtime state for the response. |
| Domain precondition failure | Interlock/result | The operation is rejected; final safety reconciliation remains in the loop. |

## Physical Output Ownership

None. The adapter may update owner-controlled state or request work, but it is
not a GPIO, PWM, SPI, or watchdog task.

## Safety and Failure Behavior

Command shape and domain preconditions are validated before mutation. The
runtime loop applies the final pending-safety reconciliation after the adapter
returns.

## Source References

- `firmware/src/bin/flux_purr/control_plane.rs`
- `firmware/src/bin/flux_purr/runtime_loop.rs`: `ControlLineContext`
- `firmware/src/bin/flux_purr/lan.rs`: `lan_command_to_control_line`
