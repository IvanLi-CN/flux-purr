# EEPROM 记忆配置实现状态

## Current Status

- Implementation: firmware uses the external M24C64 as its only persistent configuration store.
- Lifecycle: active.

## Implementation Coverage

- `firmware/src/memory.rs` currently owns the M24C64 v5 `MemoryRecord`, redundant EEPROM slots, TLV decoding, CRC validation, and bounded shared-I2C access.
- `firmware/src/bin/flux_purr.rs` reads and writes only EEPROM records; unavailable, unreadable, or unverifiable EEPROM enters `EEPROM_REQUIRED`.
- `tools/flux-purr-devd` does not stage or restore MCU internal configuration during update, flash, or recovery.

## Remaining Gaps

- Keep EEPROM record compatibility only within EEPROM; do not import internal Flash records.
- Keep firmware, host-tool, layout, and bundle tests aligned with the EEPROM-only boundary.

## References

- `./SPEC.md`
- `./HISTORY.md`
