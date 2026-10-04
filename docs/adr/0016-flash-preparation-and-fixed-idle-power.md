# Flash Preparation And Fixed Idle Power

## Status

Accepted

## Context

Flux Purr uses an FUSB302B PHY with an MCU-owned PD policy engine. An active
PPS contract needs periodic Requests, but the MCU cannot service PD while it
is held in reset or running the ROM programmer. The former idle policy preferred
12V PPS. Entering the programmer from that state can therefore expose
the shared VBUS-powered MCU and PHY to a supply interruption during Flash
work. This is a failure mechanism to prevent; it is not evidence that every
observed flashing failure has this cause.

The product needs two cooperating behaviors: an automatic transition to
Fixed power during Idle Operation, and a Device-confirmed Flash Preparation
boundary before a host takes the MCU away from the application. A responding
application must settle before Flash work; an application that cannot respond
must remain recoverable. A timeout from a responding application is not proof
that it is ready.

## Repository Facts

- [The PD specification](../specs/fusb302b-dual-pd-sink/SPEC.md) defines the
  FUSB-only product, Fixed standby selection and bounded PPS descent,
  `Accept`/`PS_RDY` contract commit, five-second PPS renewal, and independent
  physical heater revocation.
- [Power ownership](0011-exclusive-power-intent-ownership.md) and
  [terminal tickets](0013-bounded-power-commands-with-terminal-tickets.md)
  already separate product intent from PD protocol and physical outputs.
- `PdContractRequest::fixed` and the fixed-PDO selector currently reject
  operating current below 3A. That heating qualification must not prevent a
  valid low-load standby request.
- `heaterEnabled` is an intent; `heaterPhysicalOutputPercent` is applied
  heater output. `activeCoolingEnabled` is a preference; `fanEnabled` is the
  applied fan-enable state. There is no fan tachometer feedback.
- `pdContractKind=none` also represents unavailable or stale observations.
  It does not establish that an earlier PPS contract has disappeared.
- Normal post-heat cooling continues until temperature is at most 40°C and
  then for another 30 seconds. Safety-forced cooling can keep the fan running.
- [Developer Flash](../specs/firmware-update-and-developer-flash/SPEC.md)
  runs directly through the exact serial port, while product Update uses
  local devd. Both need the same preparation semantics without changing
  those execution boundaries.
- Serial configuration on macOS can reset ESP32-S3 USB Serial/JTAG. devd
  already has an unconfigured raw-descriptor path; direct EEPROM snapshot
  currently opens a separately configured `serialport` session.
- The [3V3 rail](../hardware/tps62933-dual-rail-power-design.md) and FUSB302B
  VDD depend on VBUS. Its nominal UVLO thresholds are about 4.97V rising and
  4.49V falling. The [PD RAM HIL record](../specs/pd-sink-voltage-hil/IMPLEMENTATION.md)
  includes a passing Fixed 5V terminal state, but does not validate full
  product load, MCU reset, or ROM Flash writes at 5V.

## Decision

### Automatic Idle Power

Use confirmed Fixed 5V as the preferred idle supply. A source-advertised
higher Fixed PDO that satisfies the board supply budget is an acceptable
fallback; select the lowest adequate Fixed voltage. A PPS Request for 5V
does not satisfy this policy.

The idle condition is: heating has been disarmed, physical heater
output is zero, the fan rail is disabled, and there is no active or pending
manual PPS, calibration, or thermal-test intent. A zero-duty control tick,
HOLD pause, or fan pulse gap alone is not idle. A pending heating or fan-start
request prevents a competing idle transition while its working supply is
being confirmed.

Apply the idle policy at initial source selection as well as after operation,
so an idle boot does not first create an unnecessary PPS contract. Fan-only
operation must retain an adequate working supply. A fresh heating or fan
request obtains and confirms its working supply before enabling that output;
previous requests invalidated by a fault are never replayed.

Keep the heater off during the descent. When a live PPS APDO permits it,
move toward the chosen Fixed voltage in steps of at most 500mV with at least
500ms between steps, retaining PPS renewal until Fixed is confirmed. End
with an explicit Fixed RDO and wait for `Accept` and `PS_RDY`. The final
PPS-to-Fixed change is a discrete protocol transition; it is not an analog
continuous ramp. Preserve CC/Rd, avoid a Sink Hard Reset or Type-C retoggle,
and use the existing 275ms discrete-transition boundary plus a bounded VIN
settling check. No step may leave the live APDO range.

Separate protocol-valid standby current from heating qualification. A Fixed
PDO below 3A may satisfy standby and programming demand without authorizing
heating. Use the advertised PDO, applicable cable limit, and a board-level
standby/programming budget; do not weaken the heater's current and
performance interlocks. The numerical board budget and VIN acceptance margin
need load validation before 5V becomes the unconditional preferred target.

### Device Flash Preparation

Expose a USB control operation `prepare_flash` and an idempotent status poll.
The start operation immediately disarms heat, cancels heating/calibration/test
and manual-PPS intents, clears physical heater output, and establishes an
in-memory Flash Preparation hold. It does not write these temporary changes
to EEPROM or erase saved calibration results.

While cooling is active, continue the existing fan and safety policy and keep
a supply adequate for it. Do not disable cooling to manufacture readiness.
A manually forced fan can remain a reported blocker until the Operator
resolves it. After cooling finishes, request the preferred Fixed supply, or
retain an already confirmed adequate Fixed supply when further changes are
unnecessary for programming.

The hold must govern both heater permission and Power Coordinator admission.
It rejects new heating/PPS/test requests and supersedes stale queued or
deferred requests. It also covers ownerless Idle commands and protocol
recovery, which could otherwise restore the default PPS target. Adding a
high-priority owner alone does not enforce this boundary. PD Service remains
the owner of protocol/I2C and immediate safety revocation; Flash Preparation
does not make it depend on fan or UI policy.

Hold the settled state through EEPROM backup and the handoff to the ROM
programmer. Release the hold on MCU reset or explicit cancellation, including
an Operator-facing way to cancel if the host exits. Cancellation leaves heat
disarmed and does not restore discarded requests. The hold does not expire
into a PPS or heating replay.

Return these three semantic fields in a normal request-correlated response
envelope:

```json
{
  "heating": false,
  "cooling": false,
  "pdFixedOrDefault": true
}
```

`heating` remains true until disarm is effective and applied heater output is
zero. `cooling` reports the applied fan-enable state, including required
post-heat and safety cooling; it does not report the cooling preference or
claim measured RPM. `pdFixedOrDefault` is true only for a verified Fixed
contract or a positively established ordinary Type-C default-power boundary,
with no pending or queued transition back to an MCU-serviced contract.

Unknown controller identity, missing/stale PD data, a pending request, an
unconfirmed RDO, or a remembered PPS contract makes the third field false.
Missing metadata and a VIN sample near 5V alone do not prove ordinary Type-C
power: PPS can also be near 5V. For a PD-capable source, confirming a Fixed
RDO is the preferred positive proof. The no-PD branch requires a known fresh
default-power attachment and valid supply evidence; an inherited attachment
after MCU restart must not be classified as default from missing history.

All three fields derive from one Device snapshot after output application and
protocol observation. A command ACK is not a readiness snapshot. Readiness is
`!heating && !cooling && pdFixedOrDefault` and must be rechecked immediately
before an operation that can reset the MCU. Source faults and pending
transitions revoke readiness while the hold stays active.

### Host Preparation Gate

Place the gate after artifact/confirmation/authorization checks and exact-port
locking, but before every operation that can reset the MCU, including ROM
probes. Reuse a non-resetting serial session for the initial application probe,
Flash Preparation polling, and EEPROM backup; do not reopen the port with
termios/flush between these stages.

The shared host policy is used by direct CLI Flash/Recovery and devd
Update/Recovery/Flash entry points. Direct CLI operations remain independent
of a running devd, and existing bundle verification, exact-port identity,
EEPROM rules, and General User temperature checks retain their separate
meaning.

| Observed condition | Host behavior |
| --- | --- |
| Fresh matching response satisfies all three fields | Keep the hold, complete other required preflight, recheck readiness, then enter the programmer. |
| Application responds but one field is not ready | Keep polling and display the actual blocker; never start a resetting ROM probe. |
| Application responds with unsupported command, missing fields, or protocol error | Classify it as responding but incompatible; do not turn a correlated-request timeout into an offline bypass. A legacy fallback would need equivalent positive state and hold guarantees. |
| The exact present/openable port gives no valid application response after bounded retries | Record `application_unresponsive`; skip waiting for the power-readiness gate, without claiming the Device was ready. |
| Application remains responsive and unready until the preparation deadline | Stop before reset/write and request manual download mode on the same authorized port. |
| Port disappears, changes identity, is occupied, or cannot be opened | Report the exact port failure; do not select a replacement or claim an application-unresponsive pass. |

The unresponsive branch bypasses only the power-readiness wait. Normal
Developer Flash still requires a verified EEPROM backup, or the explicit
paired backup waiver. Without either, the operation stops before Flash work.
An unresponsive application does not automatically authorize an unbacked
Flash.

Read-only application identity/status responses and `startup_busy` prove
liveness even if the preparation command is not understood. Track such
evidence separately from request correlation, including legacy error frames
without a request ID. If a previously responding application later stops
responding, reconfirm the exact port and run the same bounded application
probe before using the unresponsive branch.

Host defaults are a two-second preparation poll interval and a
ten-minute responsive preparation budget. Use repeated bounded probes for
the unresponsive classification rather than one lost reply. The long budget
accounts for real cooling; a short negotiation timeout must not be mistaken
for the whole thermal preparation deadline. A configurable wait budget can
make long cooling explicit without offering a force-flash bypass for a
responding, unready Device.

`--skip-backup --confirm NO_EEPROM_BACKUP` concerns EEPROM backup. It must not
skip preparation of an application that can respond. The paired waiver skips
ROM-mode probing and EEPROM snapshot/archive work, while retaining the
non-resetting application probe and the preparation gate. It has no ROM-mode
precondition. Report a skipped backup explicitly and keep EEPROM health
unknown when no device-side evidence exists.

## Accepted Product Decisions

Automatic idle is determined from the complete operation state. A temporary
zero heater output, HOLD pause, or fan pulse gap does not end an active
operation. This preserves manual PPS and calibration intent without repeated
Fixed/PPS switching. Flash Preparation explicitly ends those intents and
holds the settled state.

An unresponsive application bypasses only the power-readiness wait. Developer
Flash retains the explicit paired flags `--skip-backup --confirm NO_EEPROM_BACKUP`
for a backup waiver. Communication failure must not silently remove an
independent backup requirement. Missing an application response cannot be
reported as a created backup or as an EEPROM hardware fault.

## Consequences

A responding Device is held with heat off, required cooling completed, and
verified Fixed/default power before the programmer can reset the MCU. The
hold prevents unrelated commands or PD recovery from recreating PPS during
backup and handoff. An unresponsive application remains recoverable through
the existing explicit backup waiver.

Old firmware that replies but cannot prove and hold readiness requires
Operator intervention rather than an automatic offline classification. A
manually forced fan or persistent safety-cooling requirement can legitimately
exhaust the preparation budget. The host reports that blocker and requests
manual download mode without changing the authorized port.

Fixed 5V is the preferred target, with an adequate higher Fixed PDO as a
fallback. Its standby/programming current budget and VIN acceptance margin
remain hardware-validation obligations. A passing low-load RAM HIL does not
establish stability under the full product and ROM programming load.

## Verification Obligations

- Unit-level state and protocol scenarios cover idle versus zero-duty/HOLD,
  required fan cooling, cancellation, queued commands, ownerless Idle,
  recovery while held, and prevention of a stale PPS or heater replay.
- Request/response scenarios cover pending `Accept`/`PS_RDY`, an old PPS with
  missing metadata, a low-current Fixed PDO, verified non-PD default power,
  and failure to meet the supply settling boundary.
- Host scenarios cover all flashing entry points, online-but-unready,
  online-but-unsupported legacy firmware, `startup_busy`, no application
  response, loss of response after preparation, backup failure, process
  cancellation, and exact-port loss or identity change.
- Separately authorized HIL covers the full product load at Fixed 5V, the
  fallback Fixed voltage, retained CC/Rd through MCU reset, cooling followed
  by the PPS descent, and an actual ROM Flash write long enough to cross the
  former PPS renewal deadline. No MCU operation is authorized by this ADR.
- The firmware, host gate, CLI/devd contracts, and aligned specifications
  implement the accepted decisions. Full product and ROM-load hardware
  validation remains outstanding and requires separately authorized HIL.
