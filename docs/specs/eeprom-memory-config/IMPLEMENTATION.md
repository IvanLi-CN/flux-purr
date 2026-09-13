# EEPROM 记忆配置实现状态

## Current Status

- Implementation: firmware uses the external M24C64 as its only persistent configuration store.
- Lifecycle: active.

## Implementation Coverage

- `firmware/src/memory.rs` owns the FPR2 header, domain models, fixed layout, TLV codec, CRC validation, and legacy FPM1 decode compatibility.
- `firmware/src/bin/flux_purr.rs` commits changed domains under a bounded FPR2 transaction. It writes a `COMMIT/PREPARED` layout marker, writes and verifies double-slot safety domains, writes and verifies single-slot domains, then publishes the `COMMIT/ACTIVE` marker as the final publication step. Startup ignores records newer than the latest active generation, so an interrupted commit cannot mix a new curve with an older thermal plant; a transaction-id mismatch also keeps the heater locked.
- Legacy v1-v5 data is migrated through `LEGACY_MIGRATION/PREPARED` and `LEGACY_MIGRATION/ACTIVE` layout markers. FPM1 magic is invalidated only after every new domain verifies, and an interrupted migration falls back to legacy while FPM1 remains present or promotes only a complete prepared generation after invalidation.
- `ControlPlaneStatus.persistenceFault` / `persistenceFaultAttentionPending` and `InstallStatus.lastPersistenceFault` retain their existing fields. Fault `code` identifies the domain and `slot` is `A`, `B`, or `single`; ordinary-domain failures remain local and the EEPROM error page owns explicit retry.
- The `ThermalPlant` FPR2 payload keeps the fixed 768-byte slot by encoding each transient sample in 5 bytes: the low 15 bits of `elapsed_ticks` retain time and bit 15 records non-zero duty. Legacy FPM1 transient samples remain decoded from their 6-byte representation; records outside the 15-bit persisted time range are rejected before commit.
- `tools/flux-purr-devd` does not stage or restore MCU internal configuration during update, flash, or recovery.

## Remaining Gaps

- Keep EEPROM record compatibility only within EEPROM; do not import internal Flash records.
- Keep firmware, host-tool, layout, and bundle tests aligned with the EEPROM-only boundary.

## References

- `./SPEC.md`
- `./HISTORY.md`
