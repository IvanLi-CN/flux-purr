# Firmware Runtime Module Boundaries

## Context and Scope

- Context: the firmware runtime is split across async tasks, pure domain modules, hardware adapters, and shared state. Maintainers need one source-backed map of which boundary may read, command, publish, or directly control each part of the device.
- In scope: the firmware runtime module inventory, cross-module communication contracts, command and snapshot boundaries, safety interlocks, physical-output ownership, and source references for the ESP32-S3 application runtime.
- Out of scope: firmware behavior changes, Web or devd implementation details outside their firmware boundary, hardware design changes, and device or HIL validation.

## Terms and Interfaces

- Command: a request that may change runtime state, start a hardware operation, persist data, or revoke an output. Commands have one execution owner.
- Snapshot: a read-only projection of current state. Reading a snapshot must not execute a command or acquire physical-output ownership.
- Interlock: a safety gate that can deny, revoke, or expire an otherwise valid output request.
- Physical-output owner: the task or gate that performs the final GPIO, PWM, SPI, or watchdog register write for a resource.
- Runtime control plane: the front-panel executor represented by `run_runtime_loop`; it is the only consumer of the LAN control mailbox and the execution owner for LAN/USB product mutations.
- Source reference: a repository path plus stable module, type, function, or static name that identifies the implementation evidence for a documented boundary.

## Requirements

### REQ-FRM-001

- The repository MUST provide one discoverable, source-backed document for each documented firmware runtime module under `modules/`.
- Each module document MUST state responsibility, inputs/read sources, commands or mutations, published outputs, communication mechanism, control authority, physical-output ownership where applicable, safety behavior, and source references.
- `contracts/module-control-matrix.md` MUST remain a cross-module index and MUST link to every module document; it is not a substitute for the individual documents.

### REQ-FRM-002

- The documentation MUST distinguish commands, snapshots, safety interlocks, and physical-output ownership.
- A snapshot entry MUST identify the state it reads and MUST NOT be described as a command path. An interlock entry MUST identify which output it can deny or revoke.

### REQ-FRM-003

- The runtime-loop contract MUST identify the ordering between USB input, LAN mailbox consumption, PD snapshot application, sensor/control work, persistence, safety reconciliation, and display refresh.
- LAN and USB product mutations MUST be documented as executing through the runtime loop, while independent supervisor or hardware-service tasks retain only their stated service or interlock authority.

### REQ-FRM-004

- The LAN/HTTP contract MUST document authentication, pairing, lease, revision, bounded mailbox admission, response signaling, snapshot-read bypasses, and the runtime-loop revalidation boundary.
- The documentation MUST state that the HTTP task does not directly execute PD, heater, calibration, or EEPROM work.

### REQ-FRM-005

- The PD/power-domain contract MUST document `PowerCoordinator` admission and owner arbitration, the typed Fixed/PPS request and asynchronous ticket outcome, private `PD_SERVICE_COMMANDS` and `PD_SERVICE_SNAPSHOT` boundaries, FUSB302B I2C ownership, bounded service cadence, fresh-observation publication, and the heater permit/interlock behavior. Every accepted ticket MUST receive one terminal outcome. A superseded deferred operation MUST NOT be retried after its replacement has been admitted, and a failed in-flight contract MUST NOT allow an already-admitted same/lower-priority request or capability refresh to execute afterward.
- A replacement command MUST establish a fresh protocol transaction boundary before the PD service sends a new RDO, and stale control responses from the replaced transaction MUST NOT complete the replacement ticket. Application facades MUST submit owner requests through the coordinator rather than reuse a service snapshot ticket or report an active contract as already acquired.
- A capability-refresh ticket MUST remain pending while its capability response has led to an unresolved `Accept`/`PS_RDY` transaction; only a settled protocol state may publish its terminal result. A timed-out refresh MUST publish `TimedOut`, never a success derived from cached capabilities.
- The documentation MUST distinguish a PD request command from a PD status snapshot and from the heater PWM interlock.
- Shared power state MUST distinguish requested and confirmed active contracts, include protocol availability/phase and source capabilities, and describe replay plus event-driven updates without consumer polling.
- Bounded cross-module transports MUST identify their capacities and loss semantics; ticket terminal results MUST not be coalesced with latest-value state.

### REQ-FRM-006

- The Wi-Fi state/task contract MUST distinguish the hardware-independent provisioning state machine from the ESP networking adapter, including timeout, retry, error publication, and USB-only configuration authority.

### REQ-FRM-007

- The documentation MUST identify the final owner and safety gate for heater PWM, fan output, buzzer PWM, RGB status-light GPIO, display I/O, and watchdog feeding.
- Shared resources MUST identify which path owns the physical write and which paths may only submit requests or revoke permission.

### REQ-FRM-008

- The shared I2C/EEPROM contract MUST document the arbitration boundary between PD and EEPROM, chunked transaction behavior, persistence publication, and the `EEPROM_REQUIRED` heater-lock behavior.

### REQ-FRM-009

- The watchdog contract MUST document boot, runtime, and PD heartbeats, the condition that makes PD progress required, the feed decision, the five-second hardware timeout, and permit-expiry shutdown.
- Source references MUST point to stable repository paths and symbols rather than embedding a revision marker or a task-specific history.

## Verification

### VER-FRM-001

- Method: inspect `contracts/module-control-matrix.md` against the named source symbols and confirm every matrix entry has all required fields.
- covers: `REQ-FRM-001`, `REQ-FRM-002`, `REQ-FRM-009`
- Pass condition: a maintainer can identify whether a path is a command, snapshot, interlock, or physical-output owner without following an undocumented call chain.

### VER-FRM-002

- Method: source inspection of `run_runtime_loop`, `runtime_process_lan`, `runtime_process_usb_control_line`, the HTTP mailbox, and the response signal path.
- covers: `REQ-FRM-003`, `REQ-FRM-004`
- Pass condition: the documented runtime ordering and LAN/USB mutation ownership match the current source.

### VER-FRM-003

- Method: source inspection of `PdServiceClient`, `pd_service_task`, `WifiProvisioningMachine`, and `wifi_task_inner`, together with their host-test modules.
- covers: `REQ-FRM-005`, `REQ-FRM-006`
- Pass condition: command channels, snapshots, state transitions, timeout/retry behavior, and failure publication are described without assigning adapter work to a pure state module.

### VER-FRM-004

- Method: source inspection of `HeaterPwmGate`, `apply_fan_output`, `run_buzzer_task`, `run_status_light_task`, `persist_memory_domains`, and `mark_eeprom_required`.
- covers: `REQ-FRM-007`, `REQ-FRM-008`
- Pass condition: final physical writers and safety revocation paths are explicit, including the shared I2C boundary and EEPROM failure lock.

## Related ADRs

- [`../../adr/0001-thermal-plant-run-snapshot.md`](../../adr/0001-thermal-plant-run-snapshot.md)
- [`../../adr/0003-transport-scoped-wifi-provisioning.md`](../../adr/0003-transport-scoped-wifi-provisioning.md)
- [`../../adr/0006-single-output-buzzer-cue-arbitration.md`](../../adr/0006-single-output-buzzer-cue-arbitration.md)
- [`../../adr/0008-eeprom-only-configuration-persistence.md`](../../adr/0008-eeprom-only-configuration-persistence.md)
- [`../../adr/0009-eeprom-record-class-persistence.md`](../../adr/0009-eeprom-record-class-persistence.md)

## Visual Evidence

- None

## References

- [`./modules/`](./modules/)
- [`./IMPLEMENTATION.md`](./IMPLEMENTATION.md)
- [`./HISTORY.md`](./HISTORY.md)
- [`./contracts/module-control-matrix.md`](./contracts/module-control-matrix.md)
- [`../real-control-plane-runtime/SPEC.md`](../real-control-plane-runtime/SPEC.md)
- [`../buzzer-cue-arbitration/SPEC.md`](../buzzer-cue-arbitration/SPEC.md)
- [`../fusb302b-dual-pd-sink/SPEC.md`](../fusb302b-dual-pd-sink/SPEC.md)
- [`../../interfaces/http-api.md`](../../interfaces/http-api.md)
