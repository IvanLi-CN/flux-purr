# Power Coordinator

## Responsibility

Own the power-domain admission boundary between runtime consumers and
`PdService`. It arbitrates power intent owners, forwards at most one private
PD command at a time, joins duplicate capability refreshes, and projects
semantic power state without exposing PD protocol object positions.

## Inputs and Reads

- Typed `PowerCommand` values submitted through the bounded public facade.
- `PD_SERVICE_STATE` semantic state Watch.
- Non-lossy `PD_SERVICE_TERMINALS` reports from `PdService`.

## Commands and Mutations

`PowerCoordinatorClient` accepts `request(owner, PdContractRequest)`,
`refresh_capabilities()`, and `idle()`. Fixed requests require exact voltage
and operating current; PPS requests require the adapter's 100 mV / 50 mA
alignment. Each admitted command owns one `PowerTicket`. Admission returns
`Busy` when the six command/ticket slots are occupied. Lower-priority work
that arrives while a higher-priority owner is active resolves as
`Superseded`; duplicate refreshes join one in-flight operation. Every admitted
ticket receives one terminal outcome. A replacement command is delivered to
`PdService` before it retries any deferred operation, so an operation already
settled as `Superseded` cannot be retransmitted. If an in-flight contract
fails, already-admitted same/lower-priority requests and queued capability
refreshes settle as `Superseded`; higher-priority requests and explicit Idle
remain eligible for ordered dispatch. Application facades do not reuse a
snapshot ticket or report an already-active contract as a newly acquired owner
intent; every owner request enters this arbitration boundary. A terminal report
with a stale ticket cannot overwrite the latest-value state projection.

## Published Outputs

- `PowerState` through the public capacity-5 `Watch` subscription and a
  read-only `latest()` projection. It carries protocol/availability, requested
  contract, confirmed active contract, source capabilities, and failure state.
- One terminal `TicketOutcome` for every admitted ticket: `Confirmed`,
  `CapabilitiesRefreshed`, `Rejected`, `TimedOut`, `TransportFault`,
  `Detached`, or `Superseded`.

## Mechanism

- `POWER_COMMANDS`: bounded `Channel` capacity `6`.
- `PD_SERVICE_COMMANDS`: private `Channel` capacity `1`.
- `PD_SERVICE_TERMINALS`: bounded `Channel` capacity `6`, so terminal events
  are not represented by a lossy latest-value state channel.
- `POWER_STATE`: public `Watch` capacity `5`; `POWER_STATE_LATEST` has exactly
  one writer, the coordinator task.
- Embassy task waits on command admission, terminal reports, or semantic state
  changes; consumers subscribe with `changed().await` and do not poll another
  module for status changes.

## Control Authority

Owns arbitration, admission, ticket lifecycle, and the read-only state
projection. It does not own FUSB302B policy, physical PD I2C, heater PWM, fan
output, or protocol object selection.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| `PowerCoordinatorClient::request` | Command facade | Submits one typed Fixed/PPS intent with owner priority. |
| `PowerCoordinatorClient::refresh_capabilities` | Command facade | Explicitly requests a fresh source-capability exchange. |
| `PowerStateSubscription` / `latest()` | Snapshot | Read-only semantic state; no command or physical ownership. |
| `PowerTicket` / `TicketOutcome` | Completion | Asynchronous terminal result with one result per admitted ticket. |

## Safety and Failure Behavior

The coordinator never authorizes heat from a requested-but-unconfirmed
contract. `PdService` independently clears `PD_HEATER_PERMIT` and forces the
physical PWM gate off on source failure, so safety does not depend on a state
notification being delivered. A failed terminal state does not retain a
replayable request, and restoring power requires a new explicit request.

## Source References

- `firmware/src/bin/flux_purr/power_domain.rs`: `PowerCoordinatorClient`,
  `power_coordinator_task`, `PowerState`, `PowerTicket`, `TicketOutcome`,
  transport capacities, and the single-writer latest projection.
- `firmware/src/bin/flux_purr/pd_service.rs`: private command adapter and
  terminal/state publication.
