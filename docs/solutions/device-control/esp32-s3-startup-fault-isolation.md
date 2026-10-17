---
title: ESP32-S3 Startup and Control-Plane Fault Isolation
module: firmware
problem_type: runtime_failure
component: pd_and_usb_runtime
tags: [esp32-s3, pd, pps, usb, recovery]
status: active
related_specs:
  - ../../specs/firmware-runtime-modules/SPEC.md
  - ../../specs/fusb302b-dual-pd-sink/SPEC.md
  - ../../specs/real-control-plane-runtime/SPEC.md
---

# ESP32-S3 Startup and Control-Plane Fault Isolation

## Context

ESP32-S3 firmware can reboot or appear unresponsive during early peripheral bring-up even when the external device remains connected. ROM boot output alone does not distinguish retained-memory corruption, stack exhaustion, or a blocking bus operation.

## Durable Rules

- Treat NOLOAD heap regions as uninitialized after every software reset. Clear the complete region before registering it with the allocator.
- Retain large boot scratch allocations until dependent C-backed subsystems have created their objects. Scrub such allocations before release when their contents can resemble pointer fields.
- Decode persistence records into a caller-owned configuration object instead of returning a large configuration value from nested parsing functions.
- Construct large thermal-tuning status/event responses directly in their owned heap allocation through a non-inlinable destination-writing ABI. Initialize the event page in that destination and fill it through `events_after_into`; a by-value event page or status local inside the writer still retains a large nested stack copy. `Box::new(status_by_value())` can retain a full response temporary in each nested caller on Xtensa. Check the event-read dispatch/status/page call chain below the runtime poll as well as the task entry frames, require the critical linked symbols, and retain `32KiB` after those measured frames in addition to the task-level PD margin; a passing task-only stack check does not establish safe live event export under PD interruption.
- Keep the root `#[esp_rtos::main]` task limited to HAL and allocator setup. After that point, transfer one heap-owned boot pipeline through ordered one-shot Embassy stage tasks, with every full stage result stored in a `Box`; do not move a complete boot state through a guarded task stack or retain a type-erased aggregate boot future. Verify every actual Xtensa task poll and the Front Panel loop against the linked CPU0 stack capacity with a fixed safety margin.
- A small read-only control request can overflow the stack when its dispatch future embeds large calibration payloads. A healthy runtime-ready marker and increasing idle uptime do not rule this out. Decode the stack-guard panic and compare the summed emitted frames along the runtime/input/dispatch call chain with the linked stack; checking only the largest task frame misses this failure. Keep control futures and their large responses heap-owned behind a non-inlined boundary, allocate LAN handlers only after receiving a command, and budget USB and LAN nested paths separately, including thermal-tuning callees and priority-3 PD interruption. Keep the large LAN response body out of its mailbox poll and retain the boxed boot system through EEPROM initialization. Preserve lease/revision checks and the serialized mutation owner.
- Run display and I2C startup operations through finite deadlines. A timeout must reach an existing recovery or safe failure path rather than block the control plane indefinitely.
- When a deadline can cancel an async SPI display transaction, the SPI-device future must own CS with a drop guard. Cancellation must deassert CS even when the underlying transfer has not returned, otherwise the next PD service interval can leave the panel selected and make startup appear to reset.
- A `BlockingAsync` SPI call cannot be preempted once it starts. Split LCD frame writes into at most `512`-byte physical transfers and yield between them; split panel reset delays into at most `1ms` busy-wait slices and yield between slices without nesting an Embassy timer under the display timeout.
- Establish the active-low backlight before any blocking bus work, complete the bounded FUSB302B Sink service window, then initialize and flush the display startup frame before lower-priority output, persistence, ADC, or network work. A missing contract is a heater interlock, not permission to block the Dashboard indefinitely. See [FUSB302B PD Sink stability and recovery](fusb302b-pd-sink-stability.md) for its scheduling, shared-bus, protocol, and recovery rules.
- Keep FUSB302B service on an independent bounded cadence during display I/O, EEPROM chunks, control commands, ADC sampling, and runtime UI work. The PD task must use an immediate try-lock on a physically shared I2C bus: if EEPROM owns the bus, it skips that turn rather than waiting or consuming queued commands, then retries on the next tick. A failed observation removes heater output but does not claim a physical VBUS loss unless the controller reports the explicit powered-detach evidence. A reported low-VBUS transition still requires a short continuous confirmation before withdrawing Rd, so source voltage transitions cannot become a CC restart loop.
- Run the FUSB302B protocol and power coordinator on the priority-3 software-interrupt executor so display, persistence, network, and buzzer work cannot defer a ready PD turn. Keep the async I2C driver on the CPU where its interrupt handler was installed. The complete protocol call chain can nest on top of an interrupted Front Panel or network task, so the Xtensa build must check the combined task frames and retain at least `64KiB` of CPU0 stack for nested calls and guard space. The task compensates for its own turn duration and retries a busy shared bus after `1ms`; no interrupt priority can preempt a transaction that already holds the I2C bus.
- For the validated FUSB302BMPX PPS target, configure automatic GoodCRC and locally initiated PD headers with the explicit PD 3.0 revision path. Leaving the sink on Rev20 can make a PPS source omit usable APDOs, leaving the Device on a stale or fixed-only contract even though the source supports PPS.
- Separate protocol contract state from measured input voltage. An accepted PPS request can remain valid while source current limiting lowers VBUS. A VIN deficit cannot revoke the protocol contract or start PD recovery. The automatic heater's relative-VIN safety boundary can independently latch heater output off; it does not change protocol readiness. Use protocol events and the PHY's real VBUS status for contract decisions; correlate independent source telemetry when diagnosing a loaded PPS run. Keep VIN calibration unchanged during contract and idle-power acceptance.
- Distinguish USB packet submission from acknowledgement. With ESP32-S3 USB Serial/JTAG, `flush_tx_nb` submits the short packet by writing `WR_DONE`; calling it again while waiting can submit an empty packet and prevent completion. Submit once and then poll `serial_in_ep_data_free` without writing `WR_DONE`. A timed-out recovery marker must retain its offset and retry through bounded nonblocking steps while the host is absent. Do not resume command intake until the marker completes or replay a failed mutation.
- Promote a persisted thermal-plant transaction only when its raw trace and physical projection are complete and valid. Structurally framed records with invalid projections must decode without an active heater model.
- Emit stage-local reset and panic evidence over the available recovery transport. Correlate the last completed stage with the fault before changing unrelated subsystems.
- Emit a runtime-ready marker only after the first UI and all remaining startup work complete, immediately before entering the runtime loop. This gives a log-based reboot-loop check an unambiguous terminal startup stage.

## Validation

- Run host persistence and binary tests after changing the decode ownership model.
- Build the exact Xtensa target to prove async display and timeout type compatibility.
- Run `scripts/check-firmware-boot-stack.sh` on each product ELF. Retain at least `32768` bytes beyond the nested USB and LAN control frames for parser, serializer, conversion, and interrupt calls. Prove a regression check rejects a known overflowing ELF before accepting its result for the fixed candidate.
- On an owner-authorized exact serial port, verify that a software reset reaches runtime, uptime increases over a delayed status read, and no panic or reset loop is observed.
- Exercise USB recovery with a fake that acknowledges asynchronously, including a partial marker while the host is absent beyond the ordinary response timeout. On hardware, close all host descriptors, wait, reconnect to the same exact authorized port, and verify increasing uptime and fresh identity/status. A test that makes every flush complete immediately cannot catch repeated short-packet submission.

## References

- [PD contract and recovery boundary](../../specs/fusb302b-dual-pd-sink/SPEC.md)
- [USB bounded-response contract](../../specs/real-control-plane-runtime/SPEC.md)
- `firmware/src/bin/flux_purr/runtime_tests.rs`: VIN authority and asynchronous USB acknowledgement regressions.
- `tools/flux-purr-devd/src/product_hil_tests.rs`: product PPS current-limit, idle-power, and host-disconnect HIL fixtures.
- `firmware/src/bin/flux_purr/{lan.rs,runtime_loop.rs}` and `scripts/check-firmware-boot-stack.sh`: heap-owned control dispatch and nested control-stack checks.
