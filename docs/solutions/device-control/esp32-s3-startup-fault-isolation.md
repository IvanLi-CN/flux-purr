# ESP32-S3 Startup Fault Isolation

## Context

ESP32-S3 firmware can reboot or appear unresponsive during early peripheral bring-up even when the external device remains connected. ROM boot output alone does not distinguish retained-memory corruption, stack exhaustion, or a blocking bus operation.

## Durable Rules

- Treat NOLOAD heap regions as uninitialized after every software reset. Clear the complete region before registering it with the allocator.
- Retain large boot scratch allocations until dependent C-backed subsystems have created their objects. Scrub such allocations before release when their contents can resemble pointer fields.
- Decode persistence records into a caller-owned configuration object instead of returning a large configuration value from nested parsing functions.
- Keep the root `#[esp_rtos::main]` task limited to HAL and allocator setup. After that point, transfer one heap-owned boot pipeline through ordered one-shot Embassy stage tasks, with every full stage result stored in a `Box`; do not move a complete boot state through a guarded task stack or retain a type-erased aggregate boot future. Verify every actual Xtensa task poll and the Front Panel loop against the linked CPU0 stack capacity with a fixed safety margin.
- Run display and I2C startup operations through finite deadlines. A timeout must reach an existing recovery or safe failure path rather than block the control plane indefinitely.
- When a deadline can cancel an async SPI display transaction, the SPI-device future must own CS with a drop guard. Cancellation must deassert CS even when the underlying transfer has not returned, otherwise the next PD service interval can leave the panel selected and make startup appear to reset.
- A `BlockingAsync` SPI call cannot be preempted once it starts. Split LCD frame writes into at most `512`-byte physical transfers and yield between them; split panel reset delays into at most `1ms` busy-wait slices and yield between slices without nesting an Embassy timer under the display timeout.
- Establish the active-low backlight before any blocking bus work, complete the bounded FUSB302B Sink service window, then initialize and flush the display startup frame before lower-priority output, persistence, ADC, or network work. A missing contract is a heater interlock, not permission to block the Dashboard indefinitely.
- Keep FUSB302B service on an independent bounded cadence during display I/O, EEPROM chunks, control commands, ADC sampling, and runtime UI work. The PD task must use an immediate try-lock on a physically shared I2C bus: if EEPROM owns the bus, it skips that turn rather than waiting or consuming queued commands, then retries on the next tick. A failed observation removes heater output but does not claim a physical VBUS loss unless the controller reports the explicit powered-detach evidence. A reported low-VBUS transition still requires a short continuous confirmation before withdrawing Rd, so source voltage transitions cannot become a CC restart loop.
- Keep the complete FUSB302B protocol task on the normal Embassy executor and let that task own its fixed cadence timer. Do not run protocol polling, recovery, or physical I2C transactions in an `InterruptExecutor`: the deep protocol call chain can overwrite the guarded ProCPU stack and reset the MCU while the external PD source remains powered.
- Keep FUSB302B automatic GoodCRC and locally initiated PD headers on the PD 2.0 revision encoding. The controller documents its PD 3.0 encoding as unsupported; using it during initial negotiation can make the Source retry and remove VBUS before the Device reaches runtime.
- Promote a persisted thermal-plant transaction only when its raw trace and physical projection are complete and valid. Structurally framed records with invalid projections must decode without an active heater model.
- Emit stage-local reset and panic evidence over the available recovery transport. Correlate the last completed stage with the fault before changing unrelated subsystems.
- Emit a runtime-ready marker only after the first UI and all remaining startup work complete, immediately before entering the runtime loop. This gives a log-based reboot-loop check an unambiguous terminal startup stage.

## Validation

- Run host persistence and binary tests after changing the decode ownership model.
- Build the exact Xtensa target to prove async display and timeout type compatibility.
- On an owner-authorized exact serial port, verify that a software reset reaches runtime, uptime increases over a delayed status read, and no panic or reset loop is observed.
