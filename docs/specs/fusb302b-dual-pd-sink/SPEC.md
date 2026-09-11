# FUSB302B Dual PD Sink

## Related ADRs

None

## Context and Scope

Flux Purr production firmware targets the FUSB302BMPX PD-message PHY. The archived CH224Q
controller board remains documentation-only and is not selected, probed, or packaged by the
product build. FUSB302BMPX uses a repository-owned sink policy that selects compatible PPS APDOs
within `5V..21V` and fixed PDOs as a fallback.

- In scope: FUSB302B identification, CC attachment, source-capability discovery, contract recovery, heater interlock, and PD status semantics.
- Out of scope: heater PID and tuning-candidate parameters, thermal-tuning state, physical VBUS-current measurement, USB-IF certification, and a CH224Q product path.

## Requirements

### REQ-FUSB-IDENTITY

- Both controllers can answer at I2C address `0x22`; an ACK alone is never a valid identity signal.
- Startup performs only the FUSB302BMPX identity transaction. A valid signature requires two stable `Device ID` reads matching the documented `0x9x` format plus a readable FUSB status bank. There is no CH224Q fallback probe because the product board is FUSB-only.
- An unstable, invalid, incomplete, or read-failed signature reports `unknown`, performs zero PD writes, and keeps heater output interlocked. The Front Panel and USB runtime still reach their normal ready state for diagnostics, but `GPIO47` remains off until a valid controller and ready contract are observed. A physical shared-address collision cannot be made safe by ACK probing and requires board correction before heating use.
- FUSB302B's `GPIO7` interrupt net is reserved for a later event-driven path and shares `GPIO8`/`GPIO9` with the M24C64 EEPROM. Sink initialization retains Rd pull-downs on both CC pins, and the current policy safely polls the PHY. EEPROM record writes and their verification must release the shared bus after every bounded chunk and service the FUSB302B before the next chunk; no EEPROM write-cycle delay or success-path flash mirror may starve receive or contract recovery.

### REQ-FUSB-CONTRACT

- FUSB302BMPX selects the best usable PPS APDO covering the requested voltage within `5V..21V`; it selects the highest usable fixed PDO at or below `20V` only when no suitable APDO is present.
- Automatic idle operation requests `12V` from a usable PPS APDO. The APDO must still cover `20V @ 3A` before it qualifies for the performance tier; heater control raises the request only when its power policy requires it.
- Source capabilities, PPS RDOs, fixed RDOs, `Accept`, `PS_RDY`, detach, reset, reject, wait, and I2C faults are explicit policy states.
- Heating is authorized only after `Accept` then `PS_RDY`. Contract loss clears the authorization and heater output.
- A contract transition from pending to ready does not revive a heater arm requested while power was unavailable; that stale intent is discarded and a new explicit arm is required after readiness.
- A missing startup contract, detached source, failed controller initialization, or later contract loss is a heater-only interlock: it must not block the Dashboard or runtime-ready signal. The device may continue to expose diagnostics while heater output remains zero, and it releases the lock only after a ready contract is observed again.

### REQ-FUSB-RECOVERY
- While waiting for `Source_Capabilities`, the Sink may retry `Get_Source_Capabilities` after the bounded retry interval, but it must not initiate a PD Soft Reset or Hard Reset solely because that response is absent or late. A Sink-initiated reset can disturb the VBUS session that powers the Device and turn a recoverable contract delay into a reboot loop. A received reset, detach, reject, wait, or controller fault remains an explicit local recovery and heater-interlock event.
- A local FUSB302B `RETRY_FAIL`, an expired `Accept`/`PS_RDY` wait, or an incomplete receive FIFO timeout does not prove a detach. Each must clear contract authorization and heater output, flush only the receive FIFO, retain the existing CC attachment, and re-query `Source_Capabilities`; it must not reinitialize the PHY, restart CC toggling, or reset the PD session. Only a received PD reset may take the physical reinitialization path.
- Contract selection rejects source capabilities below `3A`. FUSB302BMPX clamps contractual current to `3A..5A`, programmable PPS voltage to the intersection of the absolute `5V..28V` guard and the selected APDO, and fixed-PDO voltage to `5V..20V`. The currently observed `5V..21V` APDO is capability data, not a global PPS limit.
- An active PPS request is renewed every five seconds without holding the shared I2C bus while waiting for a response.
- `20V @ 3A` provides at most `60W`; `20V @ 5A` provides at most `100W`. Firmware uses the negotiated limit to cap PWM-derived heater power.
- These limits are contractual. This revision has no VBUS current shunt, physical VBUS-current reading, or hardware VBUS over-current cutoff.

### REQ-FUSB-PERFORMANCE

- A ready contract at or above `20V` and `3A` is performance-guaranteed.
- A lower-voltage PPS or fixed contract may run the heater in degraded mode, with `pdPerformanceGuaranteed=false` and a visible degraded reason.
- A ready PPS `>=20V @ >=3A` contract is the FUSB302BMPX performance tier and authorizes calibration. Its fixed-PDO fallback remains heat-only.

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

- The firmware uses the public `fusb302` crate for FUSB302B physical-layer configuration, status, FIFO handling, packet transport, and read-only device identification. Flux Purr adapts its existing blocking ESP32-S3 I2C transport to the crate's async API at the transaction boundary.
- Before sink toggle, the runtime applies the PHY's default host-current setting, disables CC measurement, and selects the toggle interrupt mask. After CC attachment, it selects the attached CC pin for measurement and applies the receiver interrupt mask before packet transmission.
- Flux Purr owns controller selection, PPS/fixed contract policy, RDO selection, `Accept`/`PS_RDY` contract commit, recovery timing, and heater interlock. The FUSB302BMPX PD 3.0 GoodCRC encoding is an explicit target-hardware opt-in. Source-only validation proves framing and policy, not real-source interoperability.

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
- Pass condition: absent source capabilities and local receive faults retain CC, interlock heat, and retry discovery without initiating a Sink reset or reading a VBUS-loss condition.

### VER-FUSB-STATUS

- Method: control-plane status serialization tests.
- covers: `REQ-FUSB-STATUS`
- Pass condition: contract fields remain distinct from compatibility and physical-current telemetry.

### VER-FUSB-HARDWARE

- Method: release build, netlist inspection, and separately authorized HIL.
- covers: `REQ-FUSB-HARDWARE`
- Pass condition: the release artifact builds against the public PHY boundary, hardware evidence preserves the FUSB302B topology, and any physical-source interoperability claim is backed by an authorized HIL receipt.
