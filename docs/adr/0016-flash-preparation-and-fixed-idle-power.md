# Flash Preparation And Fixed Idle Power

## Status

Accepted

## Context

Flux Purr uses an FUSB302B PHY with an MCU-owned PD policy engine. An active
PPS contract needs periodic Requests, so the MCU cannot service PD while it is
held in reset or running the ROM programmer. A Fixed contract is therefore a
useful idle and programming supply target. This power behavior is a firmware
responsibility; it does not make a host flash command responsible for driving
the device into a particular contract.

The product also needs an explicit device operation that can stop heat and
hold the device while an operator performs a separately controlled operation.
That operation is useful to firmware callers and hardware procedures, but it
must not be silently inserted into the normal host flash path.

## Repository Facts

- [The PD specification](../specs/fusb302b-dual-pd-sink/SPEC.md) defines the
  FUSB-only product, Fixed standby selection, bounded PPS descent,
  `Accept`/`PS_RDY` contract commit, PPS renewal, and independent physical
  heater revocation.
- [Power ownership](0011-exclusive-power-intent-ownership.md) and
  [terminal tickets](0013-bounded-power-commands-with-terminal-tickets.md)
  separate product intent from PD protocol and physical outputs.
- `heaterEnabled` is an intent; `heaterPhysicalOutputPercent` is applied
  heater output. `activeCoolingEnabled` is a preference; `fanEnabled` is the
  applied fan-enable state. There is no fan tachometer feedback.
- `pdContractKind=none` can represent unavailable or stale observations. It
  does not prove that an earlier PPS contract has disappeared.
- Normal post-heat cooling continues until temperature is at most 40°C and
  then for another 30 seconds. Safety-forced cooling can keep the fan running.
- Full product load and ROM-write stability at Fixed 5V require separately
  authorized HIL evidence.

## Decision

### Automatic Idle Power

Use confirmed Fixed 5V as the preferred idle supply. A source-advertised
higher Fixed PDO that satisfies the board supply budget is an acceptable
fallback; select the lowest adequate Fixed voltage using advertised voltage,
advertised current, the cable limit, and the standby budget. A PPS Request for
5V does not satisfy this policy.

If an advertised adequate 5V Request receives `Reject`, `Wait`, or times out,
settle that attempt and use the existing bounded retry or Source Capabilities
recovery. Unchanged capabilities retain 5V as the preferred candidate. Protocol
failure alone does not blacklist 5V or select a higher Fixed PDO; a higher
candidate requires advertised capability or cable-budget insufficiency.

PD contract confirmation is a protocol fact: the requested Fixed RDO becomes
the confirmed active contract only after the matching `Accept` and `PS_RDY`
exchange. VIN measurements cannot invalidate either Fixed or PPS contracts,
disarm heat, or initiate PD recovery. A PPS source operating at its current
limit may reduce delivered voltage while the accepted request remains valid.
VIN readings, ADC calibration, measurement tolerance, and measurement
dwell do not participate in standby selection, Fixed confirmation, preparation
readiness, or preparation-disarm completion. A biased or missing VIN reading
must not cause an idle voltage increase or require calibration before these
operations. Independent source-side VBUS observations belong to HIL evidence
and do not become a device or host preparation gate.

The idle condition is: heating has been disarmed, physical heater output is
zero, the fan rail is disabled, and there is no active or pending manual PPS,
calibration, thermal-test, or output-start intent. A zero-duty control tick,
HOLD pause, or fan pulse gap alone is not idle. A pending heating or fan-start
request prevents a competing idle transition while its working supply is
being confirmed.

Apply the idle policy at initial source selection as well as after operation.
A fresh heating or fan request obtains and confirms its working supply before
enabling that output; invalidated requests are never replayed. Fan-only
operation retains its existing adequate working supply and is not weakened by
the idle contract.

When descending from PPS, keep the heater off, retain PPS renewal, move in
steps of at most 500mV with at least 500ms between steps, and finish with an
explicit Fixed RDO followed by `Accept` and `PS_RDY`. Preserve CC/Rd and avoid
Hard Reset or Type-C retoggle. The existing 275ms discrete-transition
boundary remains in force. No step may leave the live APDO range.

Separate protocol-valid standby current from heating qualification. A Fixed
PDO below 3A may satisfy standby and programming demand without authorizing
heat. The advertised PDO, cable limit, and board standby/programming budget
remain constraints; heater current and performance interlocks are unchanged.

### Explicit Device Flash Preparation

The firmware exposes `prepare_flash`, `get_flash_preparation`, and
`cancel_flash_preparation` as an explicit USB operation. `prepare_flash` is
idempotent: it disarms heat, cancels heating/calibration/test/manual-PPS
intents, clears physical heater output, and establishes an in-memory hold. It
does not write these temporary changes to EEPROM or erase saved calibration.

While cooling is active, the existing fan and safety policy continues. The
hold governs heater permission and Power Coordinator admission, rejects new
heating/PPS/test requests, and prevents stale queued or deferred requests
from replaying. PD Service remains responsible for protocol and immediate
safety revocation.

The hold has no automatic expiry. `cancel_flash_preparation` or MCU reset
clears it; cancellation leaves heat disarmed and does not restore discarded
requests. The operation returns one applied-state snapshot containing:

```json
{
  "heating": false,
  "cooling": false,
  "pdFixedOrDefault": true
}
```

`heating` remains true until disarm is effective and applied heater output is
zero. `cooling` reports the applied fan-enable state, including required
post-heat and safety cooling. `pdFixedOrDefault` is true only for a protocol-confirmed
Fixed contract or a positively established ordinary Type-C default-power
boundary with no pending transition back to an MCU-serviced contract.

Unknown identity, missing or stale PD data, a pending request, an unconfirmed
RDO, a remembered PPS contract, and a source fault make
`pdFixedOrDefault` false. A single 5V sample does not prove a Fixed contract.
All three fields come from one snapshot after output application and protocol
observation; an ACK is not a readiness snapshot.

Ordinary Type-C default power requires attachment-epoch and Rp-budget evidence
from the Type-C state. A 5V VIN reading, a reset, or missing PD metadata cannot
substitute for that evidence. Until this boundary is supported, absence of a
confirmed Fixed contract keeps `pdFixedOrDefault` false.

### Host Flash Boundary

The direct CLI and devd update/flash/recover paths do not automatically call
the device preparation operations and do not implement a preparation timeout,
readiness poll, or `application_unresponsive` bypass. They retain their
existing boundaries:

- use the exact owner-authorized serial port and never replace it after
  disappearance or re-enumeration;
- verify the local artifact, confirmation, identity, ROM/security conditions,
  and operation-specific temperature rules;
- perform the normal Developer EEPROM backup before ROM work, unless the
  explicitly paired `--skip-backup --confirm NO_EEPROM_BACKUP` bypass is used;
- keep real writes behind the existing repository authorization and
  confirmation gates.

The firmware preparation protocol remains available for an explicitly
authorized device operation or separate HIL procedure. A host flash does not
claim that the device is Fixed, cooled, or prepared merely because the
firmware supports that protocol.

## Consequences

Idle firmware can leave a complete idle product on Fixed power without
weakening the heater's working-supply rules. PPS remains available whenever a
working request needs it, and a fresh output request confirms its working
supply before enabling hardware.

An explicit device preparation call can hold heat off and expose actual
cooling and PD state to an operator or another firmware-aware caller. Normal
host flash remains independent of that optional call, so an old application or
an already-entered ROM target is not blocked by a host-only readiness timeout.
EEPROM backup, exact-port identity, artifact, ROM, and real-write rules remain
independent safety boundaries.

Fixed 5V preference, standby current budget, full product load, and ROM-write
stability remain hardware-validation obligations. Product HIL runs with the
device's original calibration, observes the source independently, and verifies
the final idle state after runtime restoration. A low-load PD test or a
sequence that temporarily changes VIN calibration does not establish those
product claims for the original device configuration.

## Verification Obligations

- Firmware tests cover complete idle versus zero-duty/HOLD, pending output
  intent, PPS-to-Fixed timing, low-current Fixed selection, and work-supply
  confirmation before heater or fan enable.
- A targeted selection regression verifies the lowest adequate higher Fixed
  fallback when advertised 5V capability is insufficient. This case is accepted
  through the regression; an unavailable physical source configuration is
  recorded as a hardware coverage limit rather than a product-HIL blocker.
- Device protocol tests cover idempotent `prepare_flash`, status snapshots,
  cancellation, queued-request rejection, cooling, stale PPS prevention, and
  the three applied fields. Fixed selection, readiness, and preparation-disarm
  completion must be invariant under biased, absent, and stale VIN samples.
- Host tests cover CLI parsing, exact-port identity retention, artifact and
  EEPROM backup boundaries, the paired backup bypass, ROM diagnostics, and
  the absence of host preparation commands, timeout arguments, or automatic
  preparation requests.
- Separately authorized HIL covers display/Wi-Fi load at Fixed 5V,
  cooling followed by PPS descent, and three actual product
  writes with normal EEPROM backup. It includes at least 15 seconds in ROM
  before writing while the external source continues to supply Fixed 5V.
  Evidence consists of device identity, protocol state, applied outputs, source
  protocol/VBUS telemetry, and backup/write logs. It does not write VIN
  calibration or require an ADC calibration step. Final runtime restoration
  must retain the preferred Fixed contract under the advertised source
  capabilities. No MCU operation is authorized by this ADR itself.
