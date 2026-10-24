# EEPROM 记忆配置实现状态

## Current Status

- Implementation: firmware uses the external M24C64 as its only persistent configuration store.
- Lifecycle: active.
- Raw maintenance retains pending automatic-idle power tickets until confirmation or a five-second caller timeout. EEPROM writes begin only after confirmation; failed or expired requests retain the maintenance lock. Focused regressions cover delayed confirmation, explicit rejection and timeout boundaries.

## Hardware Evidence

- Firmware `0.27.6-dev.71edd81` completed the repository CLI EEPROM erase operation and readback verification of all 8192 erased bytes. The flashed ELF SHA-256 is `aa38fb2a117abfd1ffcaf6ca72c6a791f40cc533d73e58d5d3ef9cbefcb9e940`; no EEPROM backup was created, as explicitly authorized.
- A cold start confirmed missing thermal model, empty heater curve, inactive saved tuning profile, and empty user ADC calibration samples. A fresh protected heating/natural-cooling calibration then persisted transaction `1346525729` with 102 ordered samples and a bound heater curve. Natural-cooling samples have zero heater duty; all 172 captured runtime observations have the fan disabled.
- On `0.27.6-dev.71edd81`, install-status readback retained `commissioningRequired=true`, `sensorState=ready` and `heaterLocked=true`. That implementation incorrectly required two user ADC samples on each channel, even when valid factory eFuse calibration was active. The firmware tuner therefore rejected startup as `thermal_model_unavailable` before collecting any tuning samples. EEPROM maintenance and fresh thermal calibration are verified; tuning from a fully erased EEPROM is not yet qualified.
- Local private evidence is retained under `firmware/target/hil-calibration-only/`. Its calibration-only seed and provenance use the fresh transaction rather than a historical tuning profile.

## Implementation Coverage

- `firmware/src/memory.rs` owns the FPR2 header, domain models, fixed layout, TLV codec, CRC validation, and legacy FPM1 decode compatibility.
- `firmware/src/bin/flux_purr.rs` commits changed domains under a bounded FPR2 transaction. It writes a `COMMIT/PREPARED` layout marker, writes and verifies double-slot safety domains, writes and verifies single-slot domains, then publishes the `COMMIT/ACTIVE` marker as the final publication step. Startup ignores records newer than the latest active generation, so an interrupted commit cannot mix a new curve with an older thermal plant; a transaction-id mismatch also keeps the heater locked.
- Legacy v1-v5 data is migrated through `LEGACY_MIGRATION/PREPARED` and `LEGACY_MIGRATION/ACTIVE` layout markers. FPM1 magic is invalidated only after every new domain verifies, and an interrupted migration falls back to legacy while FPM1 remains present or promotes only a complete prepared generation after invalidation.
- `ControlPlaneStatus.persistenceFault` / `persistenceFaultAttentionPending` and `InstallStatus.lastPersistenceFault` retain their existing fields. Fault `code` identifies the domain and `slot` is `A`, `B`, or `single`; ordinary-domain failures remain local and the EEPROM error page owns explicit retry.
- The ThermalPlant domain stores versioned whole-device boundary metadata and a format-3 model transaction together. Its five-byte fit samples pack 19-bit 50ms time, 12-bit RTD mV, eight-bit 125mV voltage and a full/off duty flag. The 21 equilibrium / 20 transient-bin boundary and 48 fit samples fit within 767 bytes; format 3 also binds the high-voltage Fixed PDO power envelope. Legacy resistance calibration and old thermal-model formats are absent; unrelated EEPROM domains retain their existing compatibility. Host tests cover capacity, timestamp range, round-trip metadata and legacy absence.
- `tools/flux-purr-devd` does not stage or restore MCU internal configuration during update, flash, or recovery.
- The firmware tuning path records a RAM preview receipt only after the candidate hash is applied, and binds its save receipt to the verified `pps5a` bank hash and FPR2 ACTIVE generation. Host tests cover matching and mismatched identities; no EEPROM-on-device tuning receipt has been accepted yet.

## Remaining Gaps

- Keep EEPROM record compatibility only within EEPROM; do not import internal Flash records.
- Keep firmware, host-tool, layout, and bundle tests aligned with the EEPROM-only boundary.
- Validate tuning `confirm-save` across an authorized real EEPROM commit, readback, interruption, and rollback before considering its receipt qualified.

## References

- `./SPEC.md`
- `./HISTORY.md`
