# EEPROM Persistence

## Responsibility

Load, migrate, verify, publish, and persist external M24C64 configuration
records and developer snapshot chunks.

## Inputs and Reads

- The EEPROM `I2c` view.
- Record headers, CRC, and layout markers.
- Runtime memory configuration and persistence retry state.

## Commands and Mutations

- Bounded record reads and writes.
- Verified maintenance writes and active/prepared layout markers.
- Deferred memory commits and USB snapshot operations.

## Published Outputs

- Restored `MemoryConfig`.
- Persistence source and record state.
- `MemoryCommitFailure` / `PersistenceFault`.
- EEPROM snapshot responses.

## Mechanism

Chunked asynchronous I2C transactions with write/readback verification. The
runtime loop owns deferred commit scheduling.

## Control Authority

Owns the EEPROM record format and I2C transaction details. The runtime loop
decides when a commit is due and reconciles the result.

## Boundary Classification

| Boundary | Classification | Contract |
| --- | --- | --- |
| Record read/write | Command | Performs external persistence work. |
| Restored memory and record state | Snapshot | Publishes persistence facts to runtime consumers. |
| `EEPROM_REQUIRED` | Interlock | Locks heater-related behavior after persistence failure. |

## Physical Output Ownership

None directly. Persistence failure can trigger the safety state that prevents
other paths from enabling heat.

## Safety and Failure Behavior

Absent, unreadable, unwritable, incompatible, or unverifiable persistence
enters `EEPROM_REQUIRED`, clears pending heat/calibration/PPS work, and sets the
heater lock reason to `PersistenceRequired`. There is no MCU Flash configuration
fallback.

## Source References

- `firmware/src/bin/flux_purr/eeprom.rs`: `read_eeprom_persist_record`,
  `write_eeprom_persist_record`, `mark_eeprom_required`
- `firmware/src/bin/flux_purr/eeprom_snapshot.rs`
- `firmware/src/bin/flux_purr/runtime_loop.rs`: `persist_memory_domains`,
  `commit_memory_config_now`
