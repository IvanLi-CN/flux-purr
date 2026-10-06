# Firmware Update And Developer Flash Implementation

## Current Status

- Implementation: direct update/flash/recover and EEPROM-only paths are implemented and locally verified.
- Lifecycle: active.
- Catalog note: release bundles are checked against the SHA-256 integrity catalog; the CLI's devd-backed commands use local CBOR control.

## Implementation Coverage

- `tools/flux-purr-devd/src/bin/flux-purr.rs` splits 一般用户 update, 开发者 flash, and recovery dispatch; devd-backed commands use local CBOR control.
- `tools/flux-purr-devd/src/main.rs` serves the native local control endpoint while retaining HTTP only for Web-to-device boundaries.
- `tools/flux-purr-devd/src/lib.rs` and `firmware_bundle.rs` enforce the four-file bundle and integrity catalog without configuration preservation.
- `firmware/src/bin/flux_purr.rs`, `firmware/partitions.csv`, and `firmware/flash-layout.json` implement EEPROM-Only Persistence and `EEPROM_REQUIRED`.
- Developer flash uses the direct-serial snapshot protocol, private raw `.bin` archives, legacy `.fpbk` direct cleanup, and bounded retention.
- The Windows current-user DACL implementation and archive permission test run in the dedicated Windows DEVD CI job; Unix mode coverage runs with the local devd tests.

## Implemented Boundaries

- Managed local devd control and explicit endpoint support are implemented.
- Bundle v2 and the release-scoped SHA-256 integrity catalog are implemented; signing fields and migration instructions are forbidden.
- Direct `--port` Developer flash and recovery command parsing and dispatch are implemented without devd or HTTP.
- Plaintext automatic EEPROM backup and retention cleanup are implemented in the host tool with Unix modes and a Windows current-user DACL. The paired explicit bypass skips ROM probing and the complete snapshot/archive path before invoking espflash.
- A deterministic fake-espflash host test covers the actual direct-flash bypass branch and proves that it makes no ROM probe or snapshot access.
- Normal direct flash requests the application snapshot before any espflash probe. macOS USB Serial/JTAG uses a raw descriptor for snapshot and legacy maintenance reads; protocol fallback keeps that descriptor open. Each chunk has its own response deadline. A fake-clock regression completes all `256` chunks over `128` seconds and the normal-flash fixture rejects any ROM probe before a successful backup.
- Product HIL covers the normal application-to-flash path with a private EEPROM archive and the ignored `direct_product_flash_rom_dwell_hil` fixture. The dwell fixture archives EEPROM first, establishes a no-stub ROM connection without resetting back to the application, holds ROM for at least `15s`, and flashes the same product ELF under continuous exact-port identity checks. An espflash `reset` invocation alone is not proof of ROM residency.
- Corrected candidate evidence is under `target/product-hil-20261006-protocol-only/`: original calibration was preserved, the product sequence ended in protocol-confirmed Fixed 5V after cooling and runtime restoration, and `direct_product_flash_rom_dwell_hil` archived `8192` EEPROM bytes, held the authorized identity in ROM for `15217ms`, observed independent source-side Fixed 5V at `5043..5044mV`, and completed the write. The subsequent normal direct flash also completed with a durable private backup. The recorded timeout attempts are retained as failures and do not count as successful writes.
- Recorded product evidence: `candidate-accepted-hil.elf` (`adf581cc89e94d6a43ac1b1a8aff3a1055032c260c19f4107b101415ef8d06e0`) completed three normal backup-and-flash runs while a temporary VIN fit was retained; the third no-stub ROM dwell was `15,253ms` with IsolaPurr `port_c` at `5.044..5.045V` and a successful write. An earlier second-run EEPROM timeout stopped before write. Identity and restoration exchanges succeeded, but restoring the original VIN fit left Fixed 20V and preparation unready. These conditional write/backup observations are not acceptance of original-configuration idle 5V or the corrected protocol-only preparation contract. Fresh candidate evidence must use the original calibration and assert final preferred-Fixed state after restoration.
- Espflash execution diagnostics retain bounded stdout and stderr, classify observed flash stages, and distinguish connection, write, verification, and finalization failures. ROM probes are bounded; connection recovery is limited to erase/reset commands, while interrupted segment writes and ROM checksums fail without an implicit retry.
- Every protected espflash process continuously revalidates the authorized USB identity. Legacy EEPROM maintenance captures and binds the same identity for its read/write/erase exchange, and runtime verification requires a `product` firmware kind in addition to version, source, build, and layout facts.
- The firmware and partition layout use EEPROM-only persistence; internal Flash configuration fallback is removed.

## References

- `./SPEC.md`
- `./HISTORY.md`
