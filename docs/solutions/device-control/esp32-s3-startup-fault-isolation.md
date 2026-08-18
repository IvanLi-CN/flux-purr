# ESP32-S3 Startup Fault Isolation

## Context

ESP32-S3 firmware can reboot or appear unresponsive during early peripheral bring-up even when the external device remains connected. ROM boot output alone does not distinguish retained-memory corruption, stack exhaustion, or a blocking bus operation.

## Durable Rules

- Treat NOLOAD heap regions as uninitialized after every software reset. Clear the complete region before registering it with the allocator.
- Retain large boot scratch allocations until dependent C-backed subsystems have created their objects. Scrub such allocations before release when their contents can resemble pointer fields.
- Decode persistence records into a caller-owned configuration object instead of returning a large configuration value from nested parsing functions.
- Run display and I2C startup operations through finite deadlines. A timeout must reach an existing recovery or safe failure path rather than block the control plane indefinitely.
- Emit stage-local reset and panic evidence over the available recovery transport. Correlate the last completed stage with the fault before changing unrelated subsystems.

## Validation

- Run host persistence and binary tests after changing the decode ownership model.
- Build the exact Xtensa target to prove async display and timeout type compatibility.
- On an owner-authorized exact serial port, verify that a software reset reaches runtime, uptime increases over a delayed status read, and no panic or reset loop is observed.
