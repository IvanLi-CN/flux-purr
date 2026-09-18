# FUSB302B Dual PD Sink

## Related ADRs

None

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
- The protected `I2C0` bus is the only shared scheduling boundary: the Front Panel owns the normal async device and the PD task owns a non-blocking device over the same async-arbitrated bus. The raw critical section protects only the mutex state transition. At the start of each PD turn, the PD device uses an immediate try-lock; if the Front Panel currently owns a transaction, PD skips that turn without waiting or consuming mailbox commands and retries on the next `5ms` tick. Once acquired, PD owns the bus for one bounded command-plus-poll turn and releases it before publishing its snapshot. EEPROM record reads/writes, raw maintenance reads/writes, snapshot reads/digest, and verification release the shared bus after every bounded chunk and never hold it during EEPROM write-cycle delay. Startup display initialization and flush retain their one-second timeout, while the Front Panel task only applies the latest PD snapshot during those operations. Each main-loop pass consumes at most `256` USB bytes and one complete USB or LAN control command; thermal control continues at its `50ms` cadence using the latest snapshot and does not own PD protocol liveness. When the snapshot reports an unavailable contract, the physical heater gate is cleared immediately by the PD task and the main loop also fails closed before lower-priority work continues.
- A `Reject` or `Wait` received before `PS_RDY` cancels only the pending request and returns the Sink to bounded Source_Capabilities discovery. It must not permanently enter `Fault` while the Type-C attachment remains present; an active contract is still preserved only when the response belongs to a later renegotiation.

### REQ-FUSB-CONTRACT

- FUSB302BMPX clamps programmable PPS requests to the operational `5.5V` floor and the absolute `28V` PD ceiling before intersecting the selected live APDO. It selects the best usable APDO at that resulting voltage, or the highest usable fixed PDO at or below the effective requested target and `20V` when no suitable APDO is present. Therefore automatic idle fallback remains at or below its `12V` target, while an explicit higher request may select a matching fixed PDO. The current `5V..21V` APDO is source capability data, not a global product limit.
- The FUSB302B capability bridge preserves every usable live PPS APDO. Manual PPS validation and automatic heater selection must evaluate the APDO that covers the requested voltage and current; neither may collapse a mixed source to a single highest-voltage APDO or apply the archived CH224Q `21V` ceiling.
- VIN ADC calibration must select the maximum current from the APDO covering each sweep voltage; it must not reuse an aggregate capability current after crossing an APDO boundary.
- When automatic heater control uses one current ceiling for a continuous PPS voltage interval, that ceiling must be valid at every requestable voltage in the interval. It must not use a higher current advertised only by the APDO covering the interval's upper endpoint.
- Automatic idle operation requests `12V` from a usable PPS APDO. The APDO must still cover `20V @ 3A` before it qualifies for the performance tier; heater control raises the request only when its power policy requires it.
- Source capabilities, PPS RDOs, fixed RDOs, `Accept`, `PS_RDY`, CC detach, reset, reject, wait, and I2C faults are explicit policy states.
- Heating is authorized only after `Accept` then `PS_RDY`. Contract loss clears the authorization and heater output.
- While the MCU remains powered and has a valid calibrated VIN Reading, an active contract whose voltage exceeds that reading by at least `2V` for a continuous `100ms` is treated as stale contract metadata. The runtime immediately clears contract authorization and heater output, retains the existing CC session, and returns to bounded Source_Capabilities discovery. This guard is suspended while a request is awaiting `Accept`/`PS_RDY` and for `500ms` after a request, so a normal PPS voltage transition is allowed to settle. It never infers VBUS state across an unobservable MCU power loss.
- A contract transition from pending to ready does not revive a heater arm requested while power was unavailable; that stale intent is discarded and a new explicit arm is required after readiness.
- After the bounded VBUS-restore confirmation, the runtime resets only the PD protocol engine, flushes both FIFOs, reapplies the receiver PHY configuration and interrupt masks, and resets the local transmit message ID. It preserves CC pulls and does not restart Type-C toggling before bounded Source_Capabilities discovery.
- A missing startup contract, detached source, failed controller initialization, or later contract loss is a heater-only interlock: it must not block the Dashboard or runtime-ready signal. The device may continue to expose diagnostics while heater output remains zero, and it releases the lock only after a ready contract is observed again. When a valid FUSB302B is present at startup, firmware reserves at most `750ms` before low-priority shared-I2C, display, or network initialization for the independent PD task to negotiate. The startup path reads snapshots during that window and exits immediately after a ready contract. The PD task continues its independent `5ms` cadence during display initialization and startup-frame flush. An unknown or failed controller does not consume that window.

### REQ-FUSB-RECOVERY
- While waiting for `Source_Capabilities`, the Sink may retry `Get_Source_Capabilities` after the bounded retry interval, but it must not initiate a PD Soft Reset or Hard Reset solely because that response is absent or late. A Sink-initiated reset can disturb the current source attachment and turn a recoverable contract delay into a reset loop. A received reset, detach, reject, wait, or controller fault remains an explicit local recovery and heater-interlock event.
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
- Flux Purr owns controller selection, PPS/fixed contract policy, RDO selection, `Accept`/`PS_RDY` contract commit, recovery timing, and heater interlock. FUSB302BMPX startup uses the documented PD 2.0 revision encoding for automatic GoodCRC and every locally initiated control/data header; it does not use the controller's unsupported PD 3.0 encoding. Source-only validation proves framing and policy, not real-source interoperability.

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
