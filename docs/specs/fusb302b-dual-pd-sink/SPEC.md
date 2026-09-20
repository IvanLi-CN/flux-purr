# FUSB302B Dual PD Sink

## Related ADRs

- [Power Coordinator Above PD Service](../../adr/0010-power-coordinator-above-pd-service.md)
- [Exclusive Power Intent Ownership](../../adr/0011-exclusive-power-intent-ownership.md)
- [Latest-Value Power-State Subscription](../../adr/0012-latest-value-power-state-subscription.md)
- [Bounded Power Commands With Terminal Tickets](../../adr/0013-bounded-power-commands-with-terminal-tickets.md)
- [No Contract Replay After PD Failure](../../adr/0014-no-contract-replay-after-pd-failure.md)

## Context and Scope

Flux Purr production firmware targets the FUSB302BMPX PD-message PHY. The archived CH224Q
controller board remains documentation-only and is not selected, probed, or packaged by the
product build. FUSB302BMPX uses a repository-owned sink policy that applies the absolute
`5V..28V` PD guard, then selects within the live PPS APDO and falls back to fixed PDOs.

- In scope: FUSB302B identification, CC attachment, source-capability discovery, contract recovery, heater interlock, and PD status semantics.
- Out of scope: heater PID and tuning-candidate parameters, thermal-tuning state, physical VBUS-current measurement, USB-IF certification, and a CH224Q product path.

## Requirements

### REQ-FUSB-IDENTITY

- Both controllers can answer at I2C address `0x22`; an ACK alone is never a valid identity signal.
- Startup performs only the FUSB302BMPX identity transaction. A valid signature requires two stable `Device ID` reads matching the documented `0x9x` format plus a readable FUSB status bank. There is no CH224Q fallback probe because the product board is FUSB-only.
- An unstable, invalid, incomplete, or read-failed signature reports `unknown`, performs zero PD writes, and keeps heater output interlocked. The Front Panel and USB runtime still reach their normal ready state for diagnostics, but `GPIO47` remains off until a valid controller and ready contract are observed. A physical shared-address collision cannot be made safe by ACK probing and requires board correction before heating use.
- FUSB302B's `GPIO7` interrupt net remains reserved and shares `GPIO8`/`GPIO9` with the M24C64 EEPROM. Sink initialization retains Rd pull-downs on both CC pins, and a dedicated normal-executor Embassy task owns the FUSB302B runtime, its clear-on-read interrupt polling, and every physical PD I2C transaction. The task owns the fixed `5ms` `EmbassyTimer` cadence and handles at most one mailbox request before each poll, so request traffic cannot postpone protocol liveness. Any observed `STATUS0.VBUSOK=0` immediately interlocks the heater and clears contract authorization; a reported `I_VBUSOK` transition starts only a bounded `50ms` low-VBUS confirmation for physical detach. A transient or static low level without that transition must not withdraw Rd or restart CC. Only persistent low VBUS after the transition may enter physical-detach recovery, and after that recovery the PHY waits for VBUS to return before accepting another low-VBUS confirmation. The Front Panel task receives only a snapshot and submits non-blocking requests through a bounded mailbox; it never owns FUSB302B policy state or protocol polling. EEPROM record reads/writes, raw maintenance reads/writes, snapshot reads/digest, and verification release the shared bus after every bounded chunk; no EEPROM write-cycle delay or success-path flash mirror may starve the independent PD task.
- The protected `I2C0` bus is the only shared scheduling boundary: the Front Panel owns the normal async device and the PD task owns a non-blocking device over the same async-arbitrated bus. The raw critical section protects only the mutex state transition. At the start of each PD turn, the PD device uses an immediate try-lock; if the Front Panel currently owns a transaction, PD skips that turn without waiting or consuming mailbox commands, clears the published observation and heater permit, and retries on the next `5ms` tick. Once acquired, PD owns the bus for one bounded command-plus-poll turn, reads a fresh FUSB302B status snapshot, and releases the bus only after publishing the status-verified snapshot. A snapshot is heater-authorizing only when a confirmed active contract exists and `STATUS0.VBUSOK` is set. During a pending `Accept`/`PS_RDY` renegotiation, the previously confirmed active contract remains distinct from the requested contract and may remain the legal supply envelope while the source continues to report VBUSOK; timeout, reset, detach, transport fault, invalidating capabilities, stale status, or low VBUS clears it fail-closed. EEPROM record reads/writes, raw maintenance reads/writes, snapshot reads/digest, and verification release the shared bus after every bounded chunk and never hold it during EEPROM write-cycle delay. Startup display initialization and flush retain their one-second timeout, while consumers apply state through the typed power-state subscription. Each main-loop pass consumes at most `256` USB bytes and one complete USB or LAN control command; thermal control continues at its `50ms` cadence using the latest delivered state and does not own PD protocol liveness. When the snapshot reports an unavailable contract, the physical heater gate is cleared immediately by the PD task and the main loop also fails closed before lower-priority work continues.
- A ready snapshot carries a `100ms` freshness deadline. The runtime loop and PWM gate both withdraw heater authorization when the PD service stops publishing, and the runtime loop clears the physical output when the snapshot expires. Stale-VIN interlocks use a separate latched pending flag rather than the bounded request mailbox, so queue pressure cannot lose the heater shutdown or allow an old contract to be republished before the interlock is consumed.
- The watchdog is armed before the first asynchronous boot stage and uses a monotonic boot-stage heartbeat while startup is in progress, so a stalled early stage still reaches the hardware reset deadline without resetting a healthy bounded stage. After runtime starts, PD service records a monotonic heartbeat only after each bounded poll-and-publish turn. The hardware watchdog may feed only when this heartbeat and the Front Panel runtime heartbeat have both advanced while a valid PD service is required; when controller detection has failed and PD is explicitly unavailable, the runtime heartbeat alone keeps diagnostics alive. A stalled PD owner cannot leave an electrically stable but stale contract unsupervised indefinitely. The watchdog supervisor samples the heater permit at `50ms` cadence and physically clears PWM after the `100ms` freshness deadline even if the Front Panel task does not run another control pass.
- A `Reject` or `Wait` received before `PS_RDY` cancels the pending request. If renegotiation is rejected and policy still confirms the previous active contract, that contract remains the legal supply envelope; a discovery-time rejection with no active contract remains fail-closed. It must not permanently enter `Fault` while the Type-C attachment remains present.

### REQ-FUSB-CONTRACT

- FUSB302BMPX clamps programmable PPS requests to the operational `5.5V` floor and the absolute `28V` PD ceiling before intersecting the selected live APDO. It selects the best usable APDO at that resulting voltage, or the highest usable fixed PDO at or below the effective requested target and `20V` when no suitable APDO is present. Therefore automatic idle fallback remains at or below its `12V` target, while an explicit higher request may select a matching fixed PDO. The current `5V..21V` APDO is source capability data, not a global product limit.
- The FUSB302B capability bridge preserves every usable live PPS APDO. Manual PPS validation and automatic heater selection must evaluate the APDO that covers the requested voltage and current; neither may collapse a mixed source to a single highest-voltage APDO or apply the archived CH224Q `21V` ceiling.
- VIN ADC calibration must select the maximum current from the APDO covering each sweep voltage; it must not reuse an aggregate capability current after crossing an APDO boundary.
- When automatic heater control uses one current ceiling for a continuous PPS voltage interval, that ceiling must be valid at every requestable voltage in the interval. It must not use a higher current advertised only by the APDO covering the interval's upper endpoint.
- Automatic idle operation requests `12V` from a usable PPS APDO. The APDO must still cover `20V @ 3A` before it qualifies for the performance tier; heater control raises the request only when its power policy requires it.
- Source capabilities, PPS RDOs, fixed RDOs, `Accept`, `PS_RDY`, CC detach, reset, reject, wait, and I2C faults are explicit policy states.
- Heating for a newly requested contract is authorized only after `Accept`, `PS_RDY`, and a fresh status observation. During renegotiation, the previous confirmed active contract remains the legal supply envelope until confirmation replaces it or a source/protocol failure invalidates it. Requested Contract never raises the thermal power limit.
- A PPS adjustment that remains inside the currently confirmed APDO range and preserves its operating-current request is continuous: heater control does not clear PWM while waiting for confirmation. A discrete voltage increase or mode change is handled by heater control, which clears physical PWM before submitting the request and restores it only after asynchronous confirmation. The PD service reports protocol state but does not impose a generic heating pause for contract transitions.
- The application power facade accepts one atomic Fixed/PPS request containing exact voltage and operating current, explicit capability refresh, and idle. It exposes confirmed PPS adjustment ranges and source-capability summaries, but PDO/APDO object positions remain private to the PD adapter. Accepted requests complete asynchronously through bounded tickets; consumers subscribe to semantic state changes rather than polling other modules.
- Contract loss or a source/protocol failure clears authorization and heater output independently of state-notification delivery. Recovery requires a new heater request; a failed Requested Contract is never replayed automatically.
- While the MCU remains powered and has a valid calibrated VIN Reading, an active contract whose voltage exceeds that reading by at least `2V` for a continuous `100ms` is treated as stale contract metadata. The runtime immediately clears contract authorization and heater output, retains the existing CC session, and returns to bounded Source_Capabilities discovery. This guard is suspended while a request is awaiting `Accept`/`PS_RDY` and for `500ms` after a request, so a normal PPS voltage transition is allowed to settle. It never infers VBUS state across an unobservable MCU power loss.
- A contract transition from pending to ready does not revive a heater arm requested while power was unavailable; that stale intent is discarded and a new explicit arm is required after readiness.
- After the bounded VBUS-restore confirmation, the runtime resets only the PD protocol engine, flushes both FIFOs, reapplies the receiver PHY configuration and interrupt masks, and resets the local transmit message ID. It preserves CC pulls and does not restart Type-C toggling before bounded Source_Capabilities discovery.
- A missing startup contract, detached source, failed controller initialization, or later contract loss is a heater-only interlock: it must not block the Dashboard or runtime-ready signal. The device may continue to expose diagnostics while heater output remains zero, and it releases the lock only after a ready contract is observed again. When a valid FUSB302B is present at startup, firmware reserves at most `750ms` before low-priority shared-I2C, display, or network initialization for the independent PD task to negotiate. The startup path reads snapshots during that window and exits immediately after a ready contract. The PD task continues its independent `5ms` cadence during display initialization and startup-frame flush. An unknown or failed controller does not consume that window.

### REQ-FUSB-RECOVERY
- While waiting for `Source_Capabilities`, the Sink may retry `Get_Source_Capabilities` after the bounded retry interval, but it must not initiate a PD Soft Reset or Hard Reset solely because that response is absent or late. A Sink-initiated reset can disturb the current source attachment and turn a recoverable contract delay into a reset loop. A received reset, detach, timeout, transport fault, capability invalidation, or controller fault interlocks heat. A request-level reject/wait settles that ticket; a still-confirmed previous contract remains available as the active supply envelope.
- A local FUSB302B `RETRY_FAIL`, an expired `Accept`/`PS_RDY` wait, or an incomplete receive FIFO timeout does not prove a detach. Each must clear contract authorization and heater output, invalidate cached Source Capabilities, flush only the receive FIFO, retain the existing CC attachment, and re-query `Source_Capabilities`; it must not reinitialize the PHY, restart CC toggling, or reset the PD session. `RETRY_FAIL` has precedence over every queued receive frame because the failed exchange invalidates that frame before it can change policy state. `RETRY_FAIL` remains latched until a subsequent transmit start, so recovery must consume the already-handled latch until the ordinary bounded discovery interval sends the next query. A received PD reset is a protocol event and must retain Rd, the selected CC pin, and the configured PHY; it must clear contract authorization and wait for new Source Capabilities without restarting CC toggling or software/PHY initialization. Any observed low VBUS first interlocks the heater and clears contract authorization; only a reported low-VBUS transition followed by the bounded confirmation above may enter a VBUS-restore wait. During that wait, the runtime retains the selected CC/RX session and waits for VBUS to return; it then resumes bounded Source Capabilities discovery without reinitializing the PHY or restarting CC toggling. A Source Capabilities refresh that still covers the active contract updates the cache without issuing a duplicate Request. `STATUS0.BC_LVL` reports current-level/termination evidence, not a reliable Sink detach: `BC_LVL=00`, active BMC, and any unknown status retain Rd and the current PD session. The VIN stale-contract guard uses the same CC-preserving recovery and must not start a detach path.
- Every FUSB302B policy deadline, including the initial CC-attachment observation, uses one elapsed-millisecond runtime epoch. Startup must not seed the policy with an absolute monotonic timestamp and then service it with a relative timestamp; otherwise `Source_Capabilities`, `Accept`, `PS_RDY`, and PPS renewal deadlines can cease to advance after boot.
- Contract selection rejects source capabilities below `3A`. FUSB302BMPX clamps contractual current to `3A..5A`, programmable PPS voltage to the intersection of the absolute `5V..28V` guard and the selected APDO, and fixed-PDO voltage to `5V..20V`. The currently observed `5V..21V` APDO is capability data, not a global PPS limit.
- An active PPS request is renewed every five seconds without holding the shared I2C bus while waiting for a response. An active fixed contract does not send unsolicited `Get_Source_Capabilities`; its authorization changes only on a received PD event, local transport fault, validated low VBUS, or VIN stale-contract interlock.
- `20V @ 3A` provides at most `60W`; `20V @ 5A` provides at most `100W`. Firmware uses the negotiated limit to cap PWM-derived heater power.
- These limits are contractual. This revision has no VBUS current shunt, physical VBUS-current reading, or hardware VBUS over-current cutoff.

### REQ-FUSB-PERFORMANCE

- A ready contract at or above `20V` and `3A` is performance-guaranteed.
- A lower-voltage PPS or fixed contract may run the heater in degraded mode, with `pdPerformanceGuaranteed=false` and a visible degraded reason.
- A ready PPS `>=20V @ >=3A` contract is the FUSB302BMPX performance tier and authorizes calibration. Its fixed-PDO fallback remains heat-only.
- A terminal thermal-calibration disarm remains effective across source-capability refresh, temporary fixed-PDO fallback, and PPS rediscovery. Only an explicit manual PPS re-arm may clear that disarm.

### REQ-FUSB-STATUS

Status retains `currentMa`, `ppsCapability*`, and `manualPps*` compatibility fields and adds:

- `pdController`: `fusb302b | unknown` (the legacy `ch224q` value remains decodable for transport compatibility only)
- `pdContractKind`: `fixed | pps | none`
- `pdContractCurrentMa`: negotiated upper current limit
- `pdContractPowerMw`: negotiated upper power limit
- `pdPerformanceGuaranteed`: performance-tier flag
- `pdDegradedReason`: absent when guaranteed, otherwise a finite reason

`currentMa` is not renamed and is not redefined as a measured VBUS load current.

### REQ-FUSB-HARDWARE

`docs/hardware/netlists/main-controller-board.enet` remains the archived CH224Q baseline. `docs/hardware/netlists/main-controller-board-fusb302b-rev-5-2.enet` is the imported FUSB302B source netlist.

C20 is directly `VBUS`-to-`GND`, marked `Add into BOM=yes`, and is explicitly recorded as `100uF ±20% 50V` with `Voltage Rating: 50V` and `DeviceName: C1210_100UF_50V_20%`. The imported source markings are preserved without substitution. A physical component marking is authoritative, followed by traceable assembly BOM/AOI or rework evidence for the actual board. C42 and C43 remain separately specified `100uF`, `35V` VBUS bulk capacitors.

### REQ-FUSB-DRIVER

- The firmware uses the public `fusb302` crate for FUSB302B physical-layer configuration, status, packet transport, and read-only device identification. Flux Purr uses the ESP32-S3 native async I2C driver with the crate's async API, so each physical transaction can yield on the normal executor while the shared bus remains protected. Local receive recovery writes only `CONTROL1.RX_FLUSH` through that same bounded transport and preserves the receive-mask bits; a failed transmit path additionally writes the FUSB302B `CONTROL0.TX_FLUSH` bit to discard the incomplete TX frame before bounded rediscovery.
- Before sink toggle, the runtime applies the PHY's default host-current setting, disables CC measurement, and selects the toggle interrupt mask. After CC attachment, it selects the attached CC pin for measurement and applies the receiver interrupt mask before packet transmission.
- Flux Purr owns controller selection, PPS/fixed contract policy, RDO selection, `Accept`/`PS_RDY` contract commit, recovery timing, and heater interlock. The validated FUSB302BMPX PPS path explicitly uses the driver's `PdRevision::Rev30` configuration for automatic GoodCRC and PD 3.0 revision bits on every locally initiated control/data header. The target hardware must be HIL-validated with a real PPS source; Rev20 is insufficient for this PPS path.

## Non-Goals

- Claiming USB-IF certification.
- Claiming physical VBUS-current measurement or over-current protection.
- Building or selecting a CH224Q product path.
- Any real flash, reset, serial read/write, or target-port switching without separate owner authorization.

## Verification

### VER-FUSB-IDENTITY

- Method: controller identity and fail-closed firmware unit tests.
- covers: `REQ-FUSB-IDENTITY`
- Pass condition: only a stable FUSB302B signature permits PD traffic; failed identification preserves diagnostics and interlocks heat.

### VER-FUSB-CONTRACT

- Method: FUSB302B adapter RDO, APDO, `Accept`, `PS_RDY`, detach/reset, and PPS-renewal unit tests.
- covers: `REQ-FUSB-CONTRACT`, `REQ-FUSB-PERFORMANCE`
- Pass condition: PPS/fixed selection, performance qualification, contract commit, and power caps match the bounded policy.

### VER-FUSB-RECOVERY

- Method: source-capability retry and transient-transport recovery unit tests, plus firmware binary tests.
- covers: `REQ-FUSB-RECOVERY`, `REQ-FUSB-DRIVER`
- Pass condition: absent source capabilities and local receive faults retain CC, interlock heat, and retry discovery without initiating a Sink reset; a continuous `2V` VIN deficit against an active contract is interlocked after `100ms` without a VIN-less VBUS inference; any low-VBUS observation interlocks immediately, while only a transition followed by the bounded confirmation enters detach recovery, and persistent low VBUS re-enters controlled CC discovery only once until VBUS is restored.

### VER-FUSB-STATUS

- Method: control-plane status serialization tests.
- covers: `REQ-FUSB-STATUS`
- Pass condition: contract fields remain distinct from compatibility and physical-current telemetry.

### VER-FUSB-HARDWARE

- Method: release build, netlist inspection, and separately authorized HIL.
- covers: `REQ-FUSB-HARDWARE`
- Pass condition: the release artifact builds against the public PHY boundary, hardware evidence preserves the FUSB302B topology, and any physical-source interoperability claim is backed by an authorized HIL receipt.
