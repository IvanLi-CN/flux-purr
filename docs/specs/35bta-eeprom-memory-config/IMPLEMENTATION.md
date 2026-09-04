# EEPROM 记忆配置实现状态

## Current Status

- Implementation: the current firmware still has the prohibited MCU Flash fallback and does not satisfy `EEPROM-Only Persistence`.
- Lifecycle: active.

## Implementation Coverage

- `firmware/src/memory.rs` currently owns the M24C64 v5 `MemoryRecord`, redundant EEPROM slots, TLV decoding, CRC validation, and bounded shared-I2C access.
- `firmware/src/bin/flux_purr.rs` currently also reads and writes `flux_cfg` plus legacy raw Flash slots. This behavior must be removed rather than retained as compatibility.
- `tools/flux-purr-devd` currently stages and restores internal configuration during real flash. This behavior must be removed with the partition and bundle layout references.

## Remaining Gaps

- Remove all MCU configuration persistence and `flux_cfg` migration.
- Add EEPROM blank initialization and verified `EEPROM_REQUIRED` safety behavior.
- Keep EEPROM record compatibility only within EEPROM; do not import internal Flash records.
- Update firmware, host-tool, layout, and bundle tests so no MCU configuration fallback remains.

## References

- `./SPEC.md`
- `./HISTORY.md`
