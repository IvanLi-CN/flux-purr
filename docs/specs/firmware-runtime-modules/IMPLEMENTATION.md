# Firmware Runtime Module Boundaries Implementation

## Current Status

- Implementation: source-backed individual module documents are maintained in `modules/`; `contracts/module-control-matrix.md` is the cross-module index and ownership map.
- Lifecycle: active.
- Catalog note: This topic is the canonical internal ownership map; capability-specific behavior remains owned by the related topic specs.

## Implementation Coverage

- `REQ-FRM-001` and `REQ-FRM-002`: each file in `modules/` records responsibility, read inputs, command inputs, published state, communication mechanism, authority, physical writer, safety behavior, and source references; the matrix links the complete module set and retains cross-boundary tables.
- `REQ-FRM-003`: the runtime-loop section records the actual `run_runtime_loop` order and the mailbox/USB execution boundary.
- `REQ-FRM-004`: the LAN/HTTP sections cover `NetHttpState`, `HttpGate`, `CONTROL_MAILBOX`, `CONTROL_RESPONSES`, lease and revision revalidation, and snapshot reads.
- `REQ-FRM-005`: the Power Coordinator and PD sections cover typed
  `PowerCoordinatorClient` requests, `POWER_COMMANDS` capacity `6`, private
  `PD_SERVICE_COMMANDS` capacity `1`, semantic state and terminal transports,
  `PD_SERVICE_SNAPSHOT`, `PdI2c`, `PD_HEATER_PERMIT`, stale-contract shutdown,
  one terminal outcome per accepted ticket, and replacement-before-retry
  ordering for superseded deferred PD operations, plus bounded settlement of
  queued same/lower-priority work after an in-flight contract failure. Owner
  facades always enter coordinator arbitration rather than reuse snapshot
  tickets or treat active state as a new owner acquisition. Explicit refresh
  completion remains pending through unresolved `Accept`/`PS_RDY` phases. The
  coordinator's private cancellation fence discards superseded mailbox work,
  retries a replacement after the bounded mailbox drains, and suppresses stale
  terminal projections. A second deferred admission cannot overwrite the first
  deferred command without settling its own ticket. Idle restoration does not
  return a shared one-shot pending ticket to multiple callers. New admissions
  fence older deferred work by owner priority before retry.
- `REQ-FRM-006`: the Wi-Fi sections separate `WifiProvisioningMachine` from `wifi_task_inner`, including the bounded saving/provisioning timeouts and retry terminal state.
- `REQ-FRM-007`: the hardware ownership section distinguishes runtime requests, final output writers, and revocation paths for heater, fan, buzzer, status light, display, and watchdog resources.
- `REQ-FRM-008`: the shared-bus and EEPROM sections cover `SharedI2cBus`, `I2c`, `PdI2c`, chunked M24C64 access, verified publication, and `EEPROM_REQUIRED`.
- `REQ-FRM-009`: the watchdog section covers boot/runtime/PD heartbeats, the 5,000 ms hardware timeout, the 50 ms supervisor sample, and expired heater permits.

## Source Coverage

- Runtime orchestration: `firmware/src/bin/flux_purr/{runtime.rs,runtime_assembly.rs,boot.rs,runtime_loop.rs}`.
- LAN and HTTP: `firmware/src/{net.rs,net_http.rs,lan.rs}` and `firmware/src/bin/flux_purr/lan.rs`.
- Wi-Fi state and adapter: `firmware/src/wifi_state.rs` and `firmware/src/net.rs`.
- PD and power-domain boundary: `firmware/src/bin/flux_purr/{power_domain.rs,pd_service.rs,pd_control.rs,pd_protocol.rs,support.rs,adc.rs}`.
- Persistence: `firmware/src/bin/flux_purr/{eeprom.rs,eeprom_snapshot.rs}`.
- Runtime control and output helpers: `firmware/src/bin/flux_purr/{control_plane.rs,thermal.rs,fan.rs,tasks.rs,display_io.rs,frontpanel.rs,watchdog.rs}` and `firmware/src/thermal_plant.rs`.
- Pure output domains: `firmware/src/{buzzer.rs,status_light.rs}`.
- Hardware profile: `firmware/src/board/s3_frontpanel.rs` and `docs/hardware/s3-frontpanel-baseline.md`.

## Verification

- Structural spec contract: the required topic headings, stable `REQ-FRM-*`/`VER-FRM-*` identifiers, complete `covers` links, and `Related ADRs` section were checked against the repository topic-spec procedure; this checkout does not contain the optional `bin/spec_contract_check.py` helper.
- Documentation integrity: `git diff --check` and repository-relative link/path inspection for the matrix source references.
- Source-backed behavior checks: firmware host tests covering `net_http`, `wifi_state`, `buzzer`, `status_light`, runtime safety helpers, and the board GPIO map.
- No hardware, flashing, serial, HIL, Web, or devd operation is part of this documentation topic.

## Coverage / rollout summary

- The module documents are intended to be consulted before changing a task, mailbox, snapshot, interlock, or physical-output writer; the matrix is the entry point for relationships spanning more than one module.
- Existing capability specs remain authoritative for wire payload semantics, PD sink behavior, buzzer arbitration policy, EEPROM record layout, and the real control-plane transport contract.
- The boot source currently carries a key-label naming discrepancy that is documented in the hardware ownership section. The documentation preserves both source facts and does not silently assign a behavior change.

## Remaining Gaps

- Physical validation of the documented GPIO and safety paths remains outside this topic and requires the separately authorized firmware HIL path.
- When a runtime module changes its communication mechanism or final physical writer, update the matrix and the owning capability spec in the same change.

## References

- [`./SPEC.md`](./SPEC.md)
- [`./HISTORY.md`](./HISTORY.md)
- [`./modules/`](./modules/)
- [`./contracts/module-control-matrix.md`](./contracts/module-control-matrix.md)
