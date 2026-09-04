# EEPROM-Only Configuration Persistence

## Status

Accepted

## Decision

The external M24C64 is the only persistent store for Device configuration. MCU internal Flash, NVS, raw sectors, and the removed historical `flux_cfg` partition must never store a `MemoryRecord` or any equivalent configuration fallback. Existing contents in those historical internal locations are ignored and are not migrated. Firmware removes their partition and all runtime, host-tool, and bundle-layout logic.

An EEPROM that is absent, unreadable, unwritable, or fails verification places the Device in `EEPROM_REQUIRED`: it must not claim persistence and must lock heater, calibration, Wi-Fi persistence, presets, and other operations that require configuration. A blank but working EEPROM may be initialized from the approved device profile and verified. A later hardware-qualified non-persistent default profile may provide limited baseline function only from RAM; it cannot be automatically selected merely because EEPROM access failed and cannot restore any MCU Flash persistence path.

This is a deliberate safety and ownership boundary. A configuration fallback in MCU Flash conceals a failed external persistence device, makes MCU layout changes data-bearing, and conflicts with the hardware contract that configuration is external to the MCU.

## Consequences

- Firmware update and recovery operations do not migrate, restore, or verify MCU configuration partitions.
- Host-side EEPROM archives are recovery material, not a second Device persistence backend.
- Existing devices whose only configuration is in `flux_cfg` reset to EEPROM-backed initialization or `EEPROM_REQUIRED` rather than receiving a compatibility import.
