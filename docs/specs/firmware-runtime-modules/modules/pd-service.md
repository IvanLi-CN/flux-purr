# PD Service

## Responsibility

Own FUSB302B policy state, protocol transactions, source-capability reads,
contract requests, and status observation.

## Inputs and Reads

- `PdI2c`.
- FUSB302B status and policy state.
- Source capabilities.
- `PD_SERVICE_COMMANDS`.

## Commands and Mutations

`PdServiceCommand` carries `AutomaticIdle`, `FixedVoltage(u16)`, and
`PpsVoltage(u16)` requests. Requests are non-blocking from the runtime caller
and resolve as `Pending`, `Confirmed`, or `Failed`.

## Published Outputs

- `PD_SERVICE_SNAPSHOT` with observation, capabilities, controller kind,
  service availability, stale-VIN guard state, and publication time.
- `PD_HEATER_PERMIT` when a fresh ready contract permits heating.

## Mechanism

- Bounded `PD_SERVICE_COMMANDS` capacity `8`.
- At most one command per `5 ms` service turn.
- Mutex-protected read-only snapshot.
- Sole FUSB302B protocol/I2C service.

## Control Authority

Owns FUSB302B I2C turns and PD policy. It does not own the normal heater
control loop or product UI.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| `PdServiceCommand` | Command | Requests a PD policy operation. |
| `PD_SERVICE_SNAPSHOT` | Snapshot | Reports the latest bounded observation. |
| `PD_HEATER_PERMIT` | Interlock | Allows or denies final heater PWM. |

## Physical Output Ownership

No normal heater duty writes. The service can revoke heater permission and
force the heater off when the contract is unavailable or stale.

## Safety and Failure Behavior

A ready contract plus a fresh hardware status read is required before heat is
permitted. An I2C-busy turn publishes no observation and does not count as a
PD heartbeat. Stale-contract handling latches the interlock and clears the
observation.

## Source References

- `firmware/src/bin/flux_purr/pd_service.rs`: `PdServiceCommand`,
  `PdServiceClient`, `PD_SERVICE_COMMANDS`, `PD_SERVICE_SNAPSHOT`,
  `pd_service_task`, `publish_pd_snapshot`
- `firmware/src/bin/flux_purr/pd_control.rs`
- `firmware/src/bin/flux_purr/pd_protocol.rs`
- `firmware/src/bin/flux_purr/support.rs`: `PD_HEATER_PERMIT`, `PdI2c`
