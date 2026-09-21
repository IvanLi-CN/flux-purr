# PD Service

## Responsibility

Own FUSB302B policy state, protocol transactions, source-capability reads,
contract requests, and status observation.

## Inputs and Reads

- `PdI2c`.
- FUSB302B status and policy state.
- Source capabilities.
- Private `PD_SERVICE_COMMANDS` adapter transport from `PowerCoordinator`.

## Commands and Mutations

`PdServiceCommand` carries an atomic typed `Contract` request containing
`Fixed` or `Pps` mode, exact voltage, and operating current, plus
`AutomaticIdle` and `RefreshCapabilities`. The service owns protocol-object
selection and never exposes PDO/APDO object positions. A request is accepted
through a `PowerTicket` and resolves asynchronously only after protocol
confirmation and a fresh status observation.

## Published Outputs

- Private `PD_SERVICE_STATE` semantic state and non-lossy terminal reports,
  carrying one `TicketOutcome` per accepted ticket.
- `PD_SERVICE_SNAPSHOT` with observation, capabilities, controller kind,
  service availability, stale-VIN guard state, pending operation, and
  publication time.
- `PD_HEATER_PERMIT` only while a fresh status observation confirms the active
  contract and VBUSOK. The last confirmed contract remains available during a
  pending renegotiation unless a failure invalidates it.

## Mechanism

- Bounded `PD_SERVICE_COMMANDS` capacity `1`; the `PowerCoordinator` owns the
  capacity-6 public admission queue and ticket-result slots.
- At most one command per `5 ms` service turn. A queued replacement command is
  handled before retrying deferred work, preventing stale superseded requests
  from being retransmitted. When a replacement is present, the service cancels
  the old local operation, flushes the receive FIFO, and refreshes
  `Source_Capabilities` before sending a new RDO; stale `Accept`/`PS_RDY`
  messages cannot complete the replacement ticket. An explicit refresh reports
  `CapabilitiesRefreshed` only after the protocol phase is settled;
  `WaitingForAccept` and `WaitingForPsRdy` remain pending. A refresh timeout
  reports `TimedOut` rather than using cached capabilities as success.
- Mutex-protected read-only snapshot.
- Sole FUSB302B protocol/I2C service.

## Control Authority

Owns FUSB302B I2C turns and PD policy. It does not own the normal heater
control loop or product UI.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| `PdServiceCommand` | Private command | Requests one PD policy operation from the coordinator. |
| `PD_SERVICE_SNAPSHOT` | Snapshot | Reports the latest bounded observation. |
| `PowerState` / `TicketOutcome` | Private state/completion | Publishes semantic state and one terminal result per accepted ticket. |
| `PD_HEATER_PERMIT` | Interlock | Allows or denies final heater PWM. |

## Physical Output Ownership

No normal heater duty writes. The service can revoke heater permission and
force the heater off when the contract is unavailable or stale.

## Safety and Failure Behavior

A confirmed active contract plus a fresh hardware status read is required
before heat is permitted. During renegotiation that confirmed contract remains
separate from the requested contract until the new one is confirmed. An
I2C-busy turn publishes no observation, revokes the physical permit, and does
not count as a PD heartbeat. Stale-contract handling latches the interlock and
clears the observation. Source loss, reset, timeout, transport fault, or
invalidated capabilities clears the active/requested contract and requires a
new heater request; no failed request is replayed automatically.

## Source References

- `firmware/src/bin/flux_purr/pd_service.rs`: `PdServiceClient`,
  `PD_SERVICE_SNAPSHOT`, `pd_service_task`, `publish_pd_snapshot`
- `firmware/src/bin/flux_purr/power_domain.rs`: `PowerCoordinatorClient`,
  `PowerState`, `PowerTicket`, `TicketOutcome`, `PD_SERVICE_COMMANDS`,
  `PD_SERVICE_STATE`, `PD_SERVICE_TERMINALS`, `POWER_COMMANDS`, `POWER_STATE`
- `firmware/src/bin/flux_purr/pd_control.rs`
- `firmware/src/bin/flux_purr/pd_protocol.rs`
- `firmware/src/bin/flux_purr/support.rs`: `PD_HEATER_PERMIT`, `PdI2c`
