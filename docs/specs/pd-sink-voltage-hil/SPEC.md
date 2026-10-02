# Flux Purr PD Sink Voltage HIL

## Related ADRs

- [PD Sink HIL Uses A Capability-Scoped RAM Session](../../adr/0015-pd-sink-hil-ram-session-boundary.md)

## Lifecycle

- Lifecycle: `active`
- Decision state: `approved design`, implementation complete
- Implementation state: the capability-scoped RAM image, host command, evidence writer, validators, and focused tests are present; an owner-authorized external-source diagnostic has a passing terminal receipt, while formal calibrated ADC acceptance remains a separate gate

## Goal

Define a repeatable hardware-in-the-loop (HIL) validation of the Flux Purr
FUSB302B PD Sink path. The test must prove, for each requested Fixed PDO or PPS
voltage, that the source advertises a usable exact capability, the sink can
establish the requested contract, and the device VIN Measurement Path observes
the requested voltage for a two-second hold. Every tier must produce a live
result, and every session must finish in a verified USB-C default state.

The test is deliberately narrower than product runtime power control. It
validates the PD controller, protocol policy boundary, VIN ADC path, and
recovery sequence without enabling the heater, fan, RGB, display, buttons, or
buzzer.

## Scope

### In scope

- FUSB302B identity, interrupt, I2C, PD packet, Source Capabilities, RDO, and
  contract-state behavior.
- Fixed voltage targets of `5V`, `9V`, `12V`, `15V`, and `20V`.
- PPS voltage targets from `5V` through `21V`, in exact `1V` steps.
- Selection of the highest safe contractual current that the selected source
  capability and the HIL policy can represent.
- Device-side VIN measurement and bounded voltage tolerance evaluation.
- Live per-tier progress and terminal classification in human and machine
  output.
- Direct contract transitions between tiers, cancellation, global timeout,
  port-loss handling, and final default-state verification.
- Evidence sufficient to reproduce the source capability, requested contract,
  observed voltage, timing, and final reset result.

### Out of scope

- Heater, fan, RGB, display, button, buzzer, thermal control, or other RAM
  bring-up operations.
- Product PowerCoordinator ownership, automatic thermal policy, or production
  heater permission.
- EEPROM reads/writes, firmware flash, ROM download workflows, or a production
  firmware update.
- Physical source-side current measurement. The board has no VBUS shunt or
  live current sensor; `contractCurrentMa` is a negotiated limit only.
- Unattended selection of a new serial port after re-enumeration.
- Claiming source power quality beyond the calibrated device-side VIN evidence.

## Existing Baseline

The design must preserve these repository facts until an owner-approved
implementation changes them:

- The hardware baseline is FUSB302BMPX at the shared I2C bus, with PD interrupt
  on GPIO7 and the VBUS divider feeding `VIN_ADC` on GPIO1. The populated
  `56k/5.1k` divider is the device-side VIN measurement path.
- Product PD service owns the physical FUSB302B I2C access and runs on a 5ms
  cadence. Product policy currently uses fixed requests up to 20V, a 3A..5A
  contractual current range, and a PPS operational minimum of 5.5V.
- Product PD request boundaries reject unaligned or unrepresentable requests;
  they do not round, clamp, silently switch mode, or fall back to a nearby
  voltage at the public request boundary.
- The existing RAM image has no FUSB302B dependency, does not expose GPIO7,
  and explicitly reports `pd=untouched`. Existing RAM capabilities must keep
  that contract.
- The existing RAM host command deadline is 30 seconds and the current RAM
  success validator expects `heater=off`, `pd=untouched`, and
  `eeprom=untouched`. A PD HIL capability therefore needs an explicit
  capability-specific protocol and validator rather than being disguised as
  `test_i2c` or another existing test.

The primary source facts are
`firmware/src/adapters/pd.rs`,
`firmware/src/adapters/fusb302b.rs`,
`firmware/src/bin/flux_purr/pd_service.rs`,
`firmware/ram-bringup/src/protocol.rs`,
`firmware/ram-bringup/src/main.rs`,
`docs/hardware/s3-frontpanel-baseline.md`, and
`docs/specs/fusb302b-dual-pd-sink/SPEC.md`.

## HIL Policy

### Exact voltage matrix

The host sends the following ordered matrix. The request value is exact; the
test never substitutes another voltage or mode.

| Phase | Targets | Request granularity | Required hold |
| --- | --- | --- | --- |
| Fixed PDO | `5000`, `9000`, `12000`, `15000`, `20000` mV | exact target PDO | `2000` ms |
| PPS | `5000` through `21000` mV inclusive | exact target in `1000` mV steps | `2000` ms |

The acceptance order is ascending voltage within each phase: Fixed first, then
PPS. The session establishes fixed 5V before the first tier and performs one
fixed 5V recovery after the final tier. Between tiers, it refreshes Source
Capabilities while preserving the current confirmed contract, then requests
the next exact target directly. An unsupported tier is recorded and skipped;
it is never replaced with a nearest Fixed PDO, a nearby PPS voltage, or a
different mode.

### Current target

`--current max` means the maximum current that is safe and representable for
the selected advertised object, not an arbitrary current typed by the
operator:

```text
contractCurrentMa = min(sourceAdvertisedMaxMa, 5000)
```

The selected object is usable only when `contractCurrentMa >= 3000`. The
request carries this value as follows:

- Fixed PDO: operating current and maximum current fields both equal
  `contractCurrentMa`.
- PPS APDO: the PPS current field equals `contractCurrentMa`; the requested
  voltage is the exact tier voltage.

The source capability must contain the exact target and support the calculated
current. If it does not, the tier is `unsupported` and no RDO is sent. The
summary must carry both `sourceAdvertisedMaxMa` and `contractCurrentMa` so a
reviewer can see whether the source or the product ceiling limited the request.

This policy intentionally keeps the existing 5A hardware/product ceiling. A
source advertising more than 5A is not permission to request more than 5A, and
the result must not imply that the board measured current.

### PPS 5V decision boundary

The requested matrix includes PPS `5000mV`, while the current production
`PdContractRequest::pps` boundary starts at `5500mV`. The HIL session uses an
explicit separate PPS envelope of `5000..21000mV`, still guarded by the exact
advertised APDO and the `3A..5A` current policy. This test-only envelope does
not loosen the production request boundary or enable a heater. A source that
does not advertise the exact 5V PPS capability produces `unsupported`; the
session never clamps it to 5.5V.

### Capability rules

Before every request, the session evaluates the latest Source Capabilities:

1. Decode and retain the raw PDO/APDO words and their decoded ranges.
2. Select only an object of the requested mode.
3. Require the exact fixed voltage, or an APDO range containing the exact PPS
   voltage.
4. Require the calculated current to be no greater than the advertised
   current and no less than 3A.
5. Reject a target outside the HIL envelope without touching the bus.

Source capability absence, malformed packets, a changed source identity, or a
stale capability set is not evidence of `unsupported`; it is a capability
discovery or negotiation error and must be reported separately.

## Tier Timing And Measurement

The tier clock has three distinct boundaries:

- `requestSentAt`: the exact request transmission time. It is diagnostic only.
- `contractConfirmedAt`: after `Accept` and `PS_RDY`, with a fresh FUSB status
  observation and the expected active contract.
- `holdStartedAt`: the first valid VIN sample after `contractConfirmedAt`.

The required `2000ms` hold is measured from `holdStartedAt` using the device's
monotonic timer. A request, an `Accept`, or a stale VIN value cannot consume
hold time.

The accepted sampling policy is:

- one VIN sample every `50ms` (`20Hz`);
- at least `20` valid samples during the 2-second hold;
- no valid-sample gap greater than `250ms`;
- at most three invalid ADC samples in the tier, provided the gap rule still
  holds;
- every valid sample must satisfy the tier voltage tolerance;
- the response includes sample count, invalid count, minimum, maximum, mean,
  first, and last measured mV.

The accepted initial tolerance is:

```text
toleranceMv = max(250, ceil(targetMv * 2.5%))
pass if abs(measuredMv - targetMv) <= toleranceMv
```

This is an HIL acceptance parameter, not an existing hardware accuracy claim.
The device VIN ADC is the required voltage evidence; an external meter or
power analyzer may provide optional independent evidence. All valid samples
must satisfy the rule; no settling interval is excluded from the two-second
hold.

VIN samples are device-side evidence from the calibrated VIN Measurement Path.
The result must name them `measuredVinMv`/`vinSamples`, never `measuredCurrent`
or `loadPower`.

### External-source diagnostic mode

The host may request `validateVin=false` for a diagnostic run when an
independent source-side voltage record is available. In this mode the device
does not use the VIN ADC during tier holds. It requires the negotiated PD
contract, `VBUSOK`, a two-second hold, and the normal sample cadence, while the
independent source record supplies the voltage judgment. Recovery ADC fields,
when present, are observational only and cannot make the run pass or fail.

The diagnostic outcome is `external_source_pass`, not the formal `pass`
outcome. The CLI accepts it as a successful command only when the complete PD
matrix and final recovery evidence validate; formal voltage acceptance still
requires joining that result with the independent source record, and the
calibrated device VIN ADC rules above remain a separate gate.
The CLI does not accept, parse, or machine-bind the source record; the join is
an operator-side acceptance step retained beside the PD evidence.

## Outcome Taxonomy

### Tier outcomes

| Outcome | Meaning | Request sent? | Continue? |
| --- | --- | ---: | ---: |
| `pass` | Exact contract confirmed and the full 2s VIN hold passed | yes | yes |
| `unsupported` | Exact mode/voltage/current is not advertised or is outside the approved HIL envelope | no | yes |
| `negotiation_failed` | A usable capability existed, but request/contract establishment failed | yes | yes after recovery |
| `measurement_failed` | Contract was confirmed, but VIN evidence was missing, invalid, stale, or outside tolerance | yes | yes after recovery |
| `recovery_failed` | The tier could not return to the verified default boundary | maybe | no |

`unsupported` is a capability result, not a protocol failure. It must include a
reason such as `no_exact_fixed_pdo`, `no_exact_pps_apdo`,
`source_current_below_3000ma`, or `product_policy_pps_min_5500`.

`negotiation_failed` includes Reject, Wait exhaustion, detach, protocol reset,
transport/I2C fault, contract mismatch, and negotiation deadline expiry. The
machine result carries a more specific `reason` and the protocol phase reached.

`measurement_failed` includes ADC/I2C measurement errors, stale samples,
sample-gap violations, insufficient valid samples, and voltage outside the
selected tolerance. It must include the best available sample statistics.

### Session outcomes

The session-level `overall` field is separate from tier outcomes:

- `pass`: every requested row passed and final default-state verification
  passed.
- `unsupported`: no technical failure occurred, but at least one requested
  row was unsupported; the tier table is still complete.
- `fail`: at least one tier had negotiation or measurement failure and final
  recovery passed.
- `recovery_failed`: an initial or final reset could not be verified.
- `cancelled`, `global_timeout`, `port_lost`, or `capability_discovery_failed`:
  the session did not reach a normal complete matrix.

The process exit code is non-zero for every overall value other than `pass`.
The machine summary remains available whenever the host received a terminal
summary; a missing summary is itself a transport failure.

## Session State Machine

```text
Idle
  -> Preflight
  -> DiscoverCapabilities
  -> FixedTierReady
  -> Requesting
  -> AwaitingContract
  -> Holding
  -> ClassifyTier
  -> NextTier
  -> PpsTierReady
  -> Requesting ...
  -> FinalReset
  -> VerifyDefault
  -> Complete
```

Any state can enter `Cancelled`, `GlobalTimeout`, or `PortLost`. Those states
must still attempt `ResetToDefault` while the authorized serial identity and
bus remain usable. A reset that cannot be verified changes the terminal state
to `recovery_failed`.

### State contracts

- `Preflight`: verify the exact authorized USB identity, RAM image identity,
  FUSB302B read-only identity, GPIO7 interrupt path, I2C access, VIN ADC
  calibration, and safe outputs. Heater must be low before the first bus write.
- `DiscoverCapabilities`: obtain a fresh Source Capabilities snapshot within
  the setup deadline. Preserve raw and decoded data for evidence.
- `*TierReady`: evaluate capability rules. Emit `unsupported` without an RDO
  when the exact object is absent.
- `Requesting`/`AwaitingContract`: accept only the requested mode, object,
  voltage, and current. `Accept` alone does not start the hold; only
  `PS_RDY` plus fresh status does.
- `Holding`: sample VIN for exactly 2s of monotonic hold time and emit progress
  without driving any product output.
- `ClassifyTier`: produce one terminal tier record, keep the confirmed contract
  until the next exact request, and never replay a failed request.
- `ResetToDefault`: use only for the initial attached-session boundary and the
  terminal cleanup boundary. It cancels pending work, prevents stale
  completion, preserves the attached CC/Rd session, re-applies the receive
  PHY, and negotiates a fixed 5V recovery contract. If the initial
  advertisement window expires on a RAM-reloaded session, the sink sends one
  bounded `Get_Source_Capabilities` query. If the source returns only GoodCRC,
  the sink sends one numbered USB-PD Soft Reset using the inherited next sink
  Message ID and waits for `Accept` followed by a fresh Source Capabilities
  advertisement. After `Accept`, the sink resets its local Message ID to zero.
  It must not use the local FUSB302B PD reset, restart Type-C toggle, or
  withdraw CC/Rd.
- Before every tier, Fixed or PPS, refresh Source Capabilities while the
  previous contract remains active, select only the exact requested object,
  and send the replacement request directly. A cached capability set must not
  be reused as the boundary for a replacement request.
- `VerifyDefault`: verify no pending request, no queued RDO or stale
  `Accept`/`PS_RDY`, the controller reports the recovery protocol boundary, and
  VBUS is at the agreed USB-C default level. The accepted interpretation is an
  attached sink holding a fixed 5V recovery contract.
- `Complete`: emit the final summary and retain heater off, EEPROM untouched,
  and the verified PD default-state marker.

The session must not replay a previous requested contract after a fault. A
fresh capability boundary is required before the next request, consistent with
`docs/adr/0014-no-contract-replay-after-pd-failure.md`.

## Reset And Recovery Contract

### Between tiers

A completed tier keeps its confirmed contract until the next exact request. The
HIL does not insert a fixed 5V request between successful tiers. Before every
next tier, including a Fixed-to-PPS transition, it:

1. stops the previous hold and keeps product outputs safe;
2. sends a fresh `Get_Source_Capabilities` while preserving CC/Rd and the
   current contract;
3. selects the exact requested Fixed PDO or PPS APDO from that response;
4. sends the replacement request directly;
5. waits for `Accept`/`PS_RDY` and only then starts the next hold.

If a tier is unsupported or its request fails, the session records that result
and the next request still starts from a fresh Source Capabilities boundary. It
never replays the failed RDO and never substitutes a fixed 5V recovery request
between tiers.

The recovery path must never restart Type-C toggle. A RAM reload can leave the
source in a PD message-id session that the new sink image cannot recover. After
the initial advertisement window expires, the image sends one bounded
`Get_Source_Capabilities` query. A GoodCRC-only response triggers one explicit
numbered USB-PD Soft Reset using the inherited next sink Message ID. The image
waits for `Accept`, resets its local Message ID to zero, and then waits for the
source advertisement while CC/Rd remains asserted. The HIL evidence records
the reset and the new capability boundary. A failure to receive the post-reset
advertisement is a capability-discovery failure.

If the source is detached, the session reports the tier's negotiation failure,
then treats a verified detached/default boundary as recovery success. It does
not choose a replacement port or target.

### Final reset

Finalization is mandatory on normal completion, cancellation, timeout, and
command error. A final `pass` requires all of the following evidence:

- `activeContract` is the verified fixed 5V recovery contract and
  `pendingRequest = null`;
- no RDO, `Accept`, or `PS_RDY` from the completed session can be consumed by
  a later session;
- PD protocol engine is idle and no automatic Source Capabilities loop is
  running;
- VBUS is at the selected USB-C default boundary, with the measured value and
  tolerance recorded. The selected attached-default interpretation uses
  `5000mV +/- 250mV` sustained for at least `500ms`;
- `heater=off`, `eeprom=untouched`, and `pd=default_verified`.

The selected interpretation of “USB-C default state” is an attached sink with a
stable fixed 5V recovery contract, no pending request, and Type-C default VBUS.
This deliberately keeps the PD power path continuous; it is not interchangeable
with a Hard Reset, physical detachment, or VBUS decay to zero.

## RAM Bring-up Design

### Image boundary

The approved design uses a capability-scoped PD HIL RAM ELF rather than adding
FUSB302B protocol code to the general display/fan/RGB bring-up image. It keeps
existing `test_*` operations' `pd=untouched` guarantee and limits the new image
to:

- FUSB302B I2C and GPIO7 interrupt;
- VIN ADC on GPIO1;
- USB Serial/JTAG JSONL transport;
- heater-off safety output;
- PD HIL state machine and bounded evidence buffers.

The image can retain `firmwareKind=ram_bringup` while advertising a distinct
`test_pd_sink` capability and an image identity/build marker. The host must
select the image by capability and validate it before loading. ELF internal
RAM checks and the image budget remain mandatory; a size failure is a design
blocker, not a reason to remove protocol or safety checks.

### Reuse boundary

The HIL image must reuse the canonical pure PD domain semantics for:

- Fixed/PPS capability decoding;
- exact request validation;
- RDO encoding;
- source capability freshness;
- `Accept`/`PS_RDY` confirmation;
- no replay after reset or transport failure.

The physical adapter may need a blocking RAM wrapper around the existing
FUSB302B register/packet operations. If the current async dependency cannot be
linked into the RAM ELF, the implementation should extract a small shared
`no_std` policy/codec module and test it against the existing RDO and packet
fixtures. It must not silently duplicate or diverge from the product policy.

### RAM safety fields

For the new capability:

- progress and terminal frames always report `heater=off`;
- EEPROM remains `untouched`;
- PD is explicitly reported as `owned`, `resetting`, or
  `default_verified`, never `untouched`;
- the final frame is not terminally successful unless default verification
  passed.

Other RAM capabilities retain the existing `heater=off`, `pd=untouched`, and
`eeprom=untouched` response contract.

## devd And CLI Contract

### Command shape

The canonical acceptance command is a single session so that the device owns
the protocol state machine and can guarantee final reset even when an
individual tier fails:

```text
flux-purr --json ram-run test pd-sink \
  --port <authorized-port> \
  --elf <pd-hil-ram-elf> \
  --evidence-dir <local-evidence-dir>
```

Optional diagnostic selectors may run only `fixed`, only `pps`, or a bounded
subset, but the acceptance run uses both complete matrices and the fixed
2-second hold. The command must not accept an arbitrary current that bypasses
the HIL policy. `--reload` keeps its existing meaning and never changes the
authorized port.

The current 30-second RAM command deadline is insufficient. The acceptance
defaults are:

- capability discovery: `3000ms`;
- negotiation per supported tier: `3000ms`;
- hold: exactly `2000ms`;
- initial/final 5V recovery: `3000ms` each;
- session deadline: `210s`, with a hard upper bound of `300s`;
- host deadline: session deadline plus transport margin.

The 210-second default covers the full 22-row matrix, capability refreshes, one
initial recovery, one final recovery, and finalization. The implementation must
calculate the actual deadline from the requested matrix and reject an over-
budget command before touching the bus.

### Realtime frames

The RAM JSONL protocol adds a capability-specific progress schema. Every frame
contains `requestId`, `firmwareKind`, `capability=test_pd_sink`, and a monotonic
`sequence`:

```json
{"type":"progress","requestId":"...","firmwareKind":"ram_bringup","capability":"test_pd_sink","sequence":12,"result":{"kind":"tier","mode":"fixed","targetMv":12000,"index":3,"total":5,"status":"holding","holdElapsedMs":950,"contractMv":12000,"contractCurrentMa":5000,"measuredVinMv":11980}}
```

Required progress kinds are `session`, `capabilities`, `tier`, `sample`, and
`recovery`. Human presentation prints at least one live line when a tier
starts, one terminal line for every tier, and one line for final reset. The
machine stream includes compact samples at the 50ms cadence; the human
renderer may throttle those lines without losing them from the evidence file.

### Terminal summary

The terminal frame carries a structured summary even when a tier was
unsupported or failed. `ok` means the protocol produced a structurally valid
terminal summary; `overall` carries the test result. The host must not turn a
per-tier `unsupported` into an early transport error.

The minimum machine summary is:

```json
{
  "type": "pd_hil_summary",
  "schemaVersion": 1,
  "sessionId": "...",
  "firmwareKind": "ram_bringup",
  "buildId": "...",
  "controller": "fusb302b",
  "overall": "pass",
  "policy": {
    "fixedMv": [5000, 9000, 12000, 15000, 20000],
    "ppsMv": [5000, 6000, 7000, "...", 21000],
    "currentMode": "max",
    "currentCeilingMa": 5000,
    "minimumCurrentMa": 3000,
    "recoveryMode": "fixed_5v_contract",
    "holdMs": 2000,
    "sampleIntervalMs": 50,
    "toleranceRule": "max(250mV,2.5%)"
  },
  "tiers": [
    {
      "mode": "fixed",
      "targetMv": 5000,
      "status": "pass",
      "sourceAdvertisedMaxMa": 5000,
      "contractCurrentMa": 5000,
      "contractConfirmed": true,
      "holdMs": 2000,
      "sampleCount": 40,
      "invalidSampleCount": 0,
      "minMeasuredVinMv": 4980,
      "maxMeasuredVinMv": 5030,
      "reason": null
    }
  ],
  "finalReset": {
    "status": "pass",
    "pdProtocol": "fixed_5v",
    "activeContract": {"mode": "fixed", "voltageMv": 5000, "currentMa": 3000},
    "pendingRequest": null,
    "defaultVbusMv": 5010,
    "defaultAdcMv": 418,
    "defaultAdcRawCode": 512,
    "defaultSampleCount": 10
  }
}
```

The actual PPS array is the complete explicit list; the abbreviated example
above is illustrative only. The summary must include raw source PDO/APDO words,
decoded capabilities, per-tier timing, reasons, and the final verification
fields.

### Human output

The human stream should remain compact and line-oriented, for example:

```text
PD HIL START controller=fusb302b tiers=22 hold=2000ms current=max
PD HIL TIER 1/22 mode=fixed target=5000mV capability=pass contract=5000mV current=5000mA hold=2000ms samples=40 vin=4980..5030mV result=PASS
PD HIL TIER 2/22 mode=fixed target=9000mV result=UNSUPPORTED reason=no_exact_fixed_pdo
PD HIL TIER 8/22 mode=pps target=12000mV contract=12000mV current=5000mA hold=2000ms vin=11960..12040mV result=PASS
PD HIL RESET status=PASS pd=default_verified contract=fixed_5v vbus=5010mV
PD HIL SUMMARY overall=UNSUPPORTED pass=21 unsupported=1 negotiation_failed=0 measurement_failed=0 recovery_failed=0
```

The exact numbers are illustrative. Human `PASS` is reserved for a tier or
session whose machine status is `pass`; an `UNSUPPORTED` tier must never be
rendered as a generic `FAIL` or silently omitted.

## Evidence Contract

Each run produces, at minimum:

- the RAM image build ID, source SHA, board, protocol version, and exact
  authorized-port identity;
- FUSB302B identity reads and the interrupt/I2C preflight result;
- raw and decoded Source Capabilities for every capability boundary;
- the exact request fields and selected object position for every supported
  tier;
- `requestSentAt`, `contractConfirmedAt`, `holdStartedAt`, and
  `holdFinishedAt` for every tier;
- VIN sample statistics, ADC raw/calibrated evidence, and the configured
  tolerance rule for formal ADC runs;
- the independent source-side voltage record for `validateVin=false`
  diagnostic runs;
- every tier status and reason, including unsupported rows;
- capability-refresh records for every tier and final fixed 5V recovery
  verification;
- the terminal machine summary and the human CLI transcript.

For formal ADC runs, the host must write an NDJSON event file containing the
raw 50ms sample events and a final JSON summary beneath an explicit
operator-selected evidence directory. An external-source diagnostic may omit
device ADC sample events and must instead retain the independent source-side
voltage record beside the PD evidence. The evidence directory is a local
artifact path, not a device write path. Sensitive serial identity data must be
redacted when the artifact leaves the local validation environment.

## Confirmed Design Decisions

- PPS HIL targets use a separate exact `5000..21000mV` envelope; production PPS
  remains `>=5500mV`.
- Maximum current is `min(sourceAdvertisedMaxMa, 5000)` with a `3000mA`
  minimum. It is a negotiated limit, never measured load current.
- Final reset verifies an attached sink at Type-C default VBUS with a stable
  fixed 5V recovery contract and no pending request; the candidate window is
  `5000mV +/- 250mV` for `500ms`.
- Formal device VIN ADC evidence is required. External electrical measurement
  is optional independent evidence for formal runs and is the required voltage
  evidence for `validateVin=false` diagnostic runs.
- Initial setup and terminal cleanup use fixed 5V recovery; tiers transition
  directly through fresh Source Capabilities boundaries without an inserted 5V
  request.
- PD HIL uses a dedicated capability-scoped RAM ELF and one device-owned,
  long-lived `test_pd_sink` session with cancellation checkpoints.
- Raw 50ms samples and the aggregate summary are both retained in evidence;
  human output may be throttled.
- Any unsupported tier is explicit and the overall session is not `pass`; the
  process exits non-zero unless every tier and final reset pass.

## Related Documents

- `docs/adr/0015-pd-sink-hil-ram-session-boundary.md`
- `docs/specs/fusb302b-dual-pd-sink/SPEC.md`
- `docs/specs/ram-bringup/SPEC.md`
- `CONTEXT.md`
