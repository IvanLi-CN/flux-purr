# HTTP Gate and LAN Mailbox

## Responsibility

Parse the LAN HTTP boundary, apply method, CORS/PNA, pairing, bearer-token,
lease, and optimistic-revision policy, then normalize authorized control writes
into a bounded command for the runtime loop.

## Inputs and Reads

- HTTP method, path, headers, and body.
- Pairing/token/lease state and the current control revision.
- Published identity/network snapshots.
- Hardware RNG entropy for pairing claims and lease IDs.

## Commands and Mutations

- Enqueue `ControlMailboxCommand` for status/event requests and product writes.
- Advance the control revision in `respond_to_command` after successful
  mutation responses.

The HTTP path never directly executes PD, heater, calibration, or EEPROM work.

## Published Outputs

- Health, pairing, identity, and network responses.
- HTTP/SSE response bodies.
- `CONTROL_MAILBOX` entries and response signals keyed by request slot/ID.

## Mechanism

- Bounded HTTP/TCP workers.
- `CONTROL_MAILBOX` capacity `4`.
- Per-worker `CONTROL_RESPONSES` and a three-second response wait.
- Read-only `HttpReadGate::Snapshot` reads published `LAN_RUNTIME` state
  without entering the mutation mailbox.

## Control Authority

Admission and response transport only. The runtime loop rechecks the exact LAN
lease and revision immediately before execution.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| `HttpGate::Dispatch` | Command admission | Enqueues an authorized bounded request. |
| `HttpReadGate::Snapshot` | Snapshot | Reads identity/network state without executing product work. |
| Lease, revision, and mailbox capacity | Interlock | Deny stale/unauthorized work or return `control_busy`. |

## Physical Output Ownership

None. HTTP owns no heater, fan, buzzer, status-light, display, or watchdog
write.

## Safety and Failure Behavior

- Method mismatch is rejected before auth/dispatch.
- Missing revision returns `revision_required`.
- Expired leases and stale revisions are rejected before hardware execution.
- A full mailbox returns `control_busy`.

## Source References

- `firmware/src/net_http.rs`: `NetHttpState`, `HttpGate`, `HttpReadGate`,
  `ControlMailboxCommand`
- `firmware/src/net.rs`: `CONTROL_MAILBOX`, `CONTROL_RESPONSES`,
  `handle_http_connection`, `respond_to_command`
