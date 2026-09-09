# EEPROM 记忆配置实现状态

## Current Status

- Implementation: firmware uses the external M24C64 as its only persistent configuration store.
- Lifecycle: active.

## Implementation Coverage

- `firmware/src/memory.rs` owns the FPR2 header, domain models, fixed layout, TLV codec, CRC validation, and legacy FPM1 decode compatibility.
- `firmware/src/bin/flux_purr.rs` writes `SafetyCalibration`, `ThermalPolicy`, `UserPreferences`, and `NetworkAndPairing` independently using EEPROM-only bounded chunks. A/B is selected only for safety-sensitive domains; preferences and network use single CRC records.
- Legacy v1-v5 data is migrated through `PREPARED` and `ACTIVE` layout markers. FPM1 magic is invalidated only after every new domain verifies, and an interrupted prepared migration remains heater-locked.
- `ControlPlaneStatus.persistenceFault` / `persistenceFaultAttentionPending` and `InstallStatus.lastPersistenceFault` retain their existing fields. Fault `code` identifies the domain and `slot` is `A`, `B`, or `single`; ordinary-domain failures remain local and the EEPROM error page owns explicit retry.
- `tools/flux-purr-devd` does not stage or restore MCU internal configuration during update, flash, or recovery.

## Remaining Gaps

- Keep EEPROM record compatibility only within EEPROM; do not import internal Flash records.
- Keep firmware, host-tool, layout, and bundle tests aligned with the EEPROM-only boundary.

## References

- `./SPEC.md`
- `./HISTORY.md`
