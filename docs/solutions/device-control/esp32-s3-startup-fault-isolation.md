# ESP32-S3 Startup Fault Isolation

## Context

ESP32-S3 firmware can reboot or appear unresponsive during early peripheral bring-up even when the external device remains connected. ROM boot output alone does not distinguish retained-memory corruption, stack exhaustion, or a blocking bus operation.

## Durable Rules

- Treat NOLOAD heap regions as uninitialized after every software reset. Clear the complete region before registering it with the allocator.
- Retain large boot scratch allocations until dependent C-backed subsystems have created their objects. Scrub such allocations before release when their contents can resemble pointer fields.
- Decode persistence records into a caller-owned configuration object instead of returning a large configuration value from nested parsing functions.
- Run display and I2C startup operations through finite deadlines. A timeout must reach an existing recovery or safe failure path rather than block the control plane indefinitely.
- Establish the active-low backlight before any blocking bus work, complete the bounded FUSB302B Sink service window, then initialize and flush the display startup frame before lower-priority output, persistence, ADC, or network work. A missing contract is a heater interlock, not permission to block the Dashboard indefinitely.
- Keep FUSB302B service on an independent bounded cadence during display I/O, EEPROM chunks, control commands, ADC sampling, and runtime UI work. A failed observation removes heater output but does not claim a physical VBUS loss unless the controller reports the explicit powered-detach evidence. A reported low-VBUS transition still requires a short continuous confirmation before withdrawing Rd, so source voltage transitions cannot become a CC restart loop.
- Keep FUSB302B automatic GoodCRC and locally initiated PD headers on the PD 2.0 revision encoding. The controller documents its PD 3.0 encoding as unsupported; using it during initial negotiation can make the Source retry and remove VBUS before the Device reaches runtime.
- Promote a persisted thermal-plant transaction only when its raw trace and physical projection are complete and valid. Structurally framed records with invalid projections must decode without an active heater model.
- Emit stage-local reset and panic evidence over the available recovery transport. Correlate the last completed stage with the fault before changing unrelated subsystems.

## Validation

- Run host persistence and binary tests after changing the decode ownership model.
- Build the exact Xtensa target to prove async display and timeout type compatibility.
- On an owner-authorized exact serial port, verify that a software reset reaches runtime, uptime increases over a delayed status read, and no panic or reset loop is observed.
